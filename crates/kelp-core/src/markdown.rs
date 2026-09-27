use pulldown_cmark::{Event, Parser, Tag};

const WRAPPER_TAGS: [&str; 8] = ["p", "div", "picture", "source", "br", "center", "span", "a"];

/// An image a Markdown file points at, resolved to a path in the repo.
#[derive(Debug, PartialEq, Eq)]
pub struct Asset {
    pub url: String,
    pub repo_path: String,
    pub width: Option<u32>,
}

pub struct Prepared {
    pub text: String,
    pub assets: Vec<Asset>,
}

/// Turns a README-style file into plain CommonMark: HTML image tags become
/// Markdown images, layout-only HTML is dropped, remote images become links.
pub fn prepare(text: &str, file_path: &str) -> Prepared {
    let mut widths = Vec::new();
    let text = outside_code(text, |prose| {
        strip_wrappers(&html_images_to_markdown(prose, &mut widths))
    });
    let dir = file_path.rsplit_once('/').map_or("", |(d, _)| d);
    let mut assets = Vec::new();
    let mut remote = Vec::new();
    for event in Parser::new(&text) {
        if let Event::Start(Tag::Image { dest_url, .. }) = event {
            let url = dest_url.to_string();
            if is_remote(&url) {
                remote.push(url);
            } else if let Some(repo_path) = resolve(dir, &url)
                && !assets.iter().any(|a: &Asset| a.url == url)
            {
                let width = widths.iter().find(|(src, _)| *src == url).map(|(_, w)| *w);
                assets.push(Asset {
                    url,
                    repo_path,
                    width,
                });
            }
        }
    }
    let mut text = text;
    for url in remote {
        text = unimage(&text, &url);
    }
    Prepared { text, assets }
}

fn outside_code(text: &str, mut prose: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut fenced = false;
    let mut pending = String::new();
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        if fenced || fence {
            out.push_str(&prose_spans(&std::mem::take(&mut pending), &mut prose));
            out.push_str(line);
            if fence {
                fenced = !fenced;
            }
        } else {
            pending.push_str(line);
        }
    }
    out.push_str(&prose_spans(&pending, &mut prose));
    out
}

fn prose_spans(text: &str, prose: &mut impl FnMut(&str) -> String) -> String {
    text.split('`')
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 0 {
                prose(part)
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("`")
}

fn unimage(text: &str, url: &str) -> String {
    let needle = format!("]({url}");
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(end) = rest.find(&needle) {
        let head = &rest[..end];
        match head.rfind("![") {
            Some(bang) => {
                out.push_str(&head[..bang]);
                out.push_str(&head[bang + 1..]);
            }
            None => out.push_str(head),
        }
        out.push_str(&needle);
        rest = &rest[end + needle.len()..];
    }
    out.push_str(rest);
    out
}

fn is_remote(url: &str) -> bool {
    url.contains("://") || url.starts_with("data:") || url.starts_with("//")
}

fn resolve(dir: &str, url: &str) -> Option<String> {
    let url = url.split(['#', '?']).next()?;
    let mut parts: Vec<&str> = if url.starts_with('/') {
        Vec::new()
    } else {
        dir.split('/').filter(|p| !p.is_empty()).collect()
    };
    for part in url.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            p => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn html_images_to_markdown(text: &str, widths: &mut Vec<(String, u32)>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = find_tag(rest, "img") {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find('>') else {
            break;
        };
        let tag = &rest[start..start + len + 1];
        match attr(tag, "src") {
            Some(src) => {
                let alt = attr(tag, "alt").unwrap_or_default();
                if let Some(width) = attr(tag, "width").and_then(|w| w.parse().ok()) {
                    widths.push((src.clone(), width));
                }
                out.push_str(&format!("![{alt}]({src})"));
            }
            None => out.push_str(tag),
        }
        rest = &rest[start + len + 1..];
    }
    out.push_str(rest);
    out
}

fn strip_wrappers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'scan: while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let name_start = usize::from(after.starts_with('/'));
        for name in WRAPPER_TAGS {
            let candidate = &after[name_start..];
            let boundary = candidate
                .get(name.len()..)
                .and_then(|s| s.chars().next())
                .is_some_and(|c| c == '>' || c == ' ' || c == '/' || c == '\n');
            if candidate.len() >= name.len()
                && candidate[..name.len()].eq_ignore_ascii_case(name)
                && boundary
                && let Some(close) = after.find('>')
            {
                rest = &after[close + 1..];
                continue 'scan;
            }
        }
        out.push('<');
        rest = after;
    }
    out.push_str(rest);
    out
}

fn find_tag(text: &str, name: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(&format!("<{name}")) {
        let at = from + i;
        let next = lower[at + name.len() + 1..].chars().next();
        if matches!(next, Some(' ' | '/' | '>' | '\n')) {
            return Some(at);
        }
        from = at + 1;
    }
    None
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        let before_ok = at == 0 || lower.as_bytes()[at - 1].is_ascii_whitespace();
        let rest = tag[at + name.len()..].trim_start();
        if before_ok && let Some(value) = rest.strip_prefix('=') {
            let value = value.trim_start();
            let quote = value.chars().next()?;
            return if quote == '"' || quote == '\'' {
                value[1..].split(quote).next().map(str::to_string)
            } else {
                value
                    .split(|c: char| c.is_whitespace() || c == '>')
                    .next()
                    .map(str::to_string)
            };
        }
        from = at + name.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_logo_becomes_a_repo_image() {
        let p = prepare(
            "<p align=\"center\"><img src=\"site/mascot.svg\" width=\"140\" alt=\"Kelp mascot\"></p>\n\n# Kelp\n",
            "README.md",
        );
        assert_eq!(p.text, "![Kelp mascot](site/mascot.svg)\n\n# Kelp\n");
        assert_eq!(
            p.assets,
            vec![Asset {
                url: "site/mascot.svg".into(),
                repo_path: "site/mascot.svg".into(),
                width: Some(140),
            }]
        );
    }

    #[test]
    fn relative_paths_resolve_from_the_file() {
        let p = prepare(
            "![a](./img/a.png) ![b](../b.png) ![c](/c.png)",
            "docs/guide/x.md",
        );
        let paths: Vec<_> = p.assets.iter().map(|a| a.repo_path.as_str()).collect();
        assert_eq!(paths, ["docs/guide/img/a.png", "docs/b.png", "c.png"]);
    }

    #[test]
    fn remote_images_become_links() {
        let p = prepare("![badge](https://img.shields.io/x.svg) text", "README.md");
        assert_eq!(p.text, "[badge](https://img.shields.io/x.svg) text");
        assert!(p.assets.is_empty());
    }

    #[test]
    fn code_and_unknown_tags_are_left_alone() {
        let text = "Use `<p>` and <kbd>Cmd</kbd>, a < b.";
        assert_eq!(prepare(text, "a.md").text, text);
        let fenced = "```html\n<p align=\"center\">x</p>\n```\n";
        assert_eq!(prepare(fenced, "a.md").text, fenced);
    }
}
