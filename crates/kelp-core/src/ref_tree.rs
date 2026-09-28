use std::collections::BTreeMap;
use std::ops::Range;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
pub enum Sort {
    #[default]
    Name,
    Newest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    pub index: usize,
    pub label: String,
    pub time: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub path: String,
    pub label: String,
    pub children: Vec<Node>,
    pub count: usize,
    pub newest: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Folder(Folder),
    Leaf(Leaf),
}

impl Node {
    fn label(&self) -> &str {
        match self {
            Node::Folder(f) => &f.label,
            Node::Leaf(l) => &l.label,
        }
    }

    fn newest(&self) -> i64 {
        match self {
            Node::Folder(f) => f.newest,
            Node::Leaf(l) => l.time,
        }
    }

    pub fn leaves(&self) -> Vec<usize> {
        let mut out = Vec::new();
        self.collect(&mut out);
        out
    }

    fn collect(&self, out: &mut Vec<usize>) {
        match self {
            Node::Leaf(l) => out.push(l.index),
            Node::Folder(f) => f.children.iter().for_each(|c| c.collect(out)),
        }
    }
}

#[derive(Default)]
struct Trie {
    leaf: Option<(usize, i64)>,
    children: BTreeMap<String, Trie>,
}

/// Groups slash-separated names into folders. A folder with a single child
/// is folded into it (`feat/solo` stays one row, `feat/ui` becomes one
/// folder), so a lone branch never costs an extra click. Folders come
/// before leaves; both follow `sort`.
pub fn build(names: &[(&str, i64)], sort: Sort) -> Vec<Node> {
    let mut root = Trie::default();
    for (index, (name, time)) in names.iter().enumerate() {
        let mut node = &mut root;
        for part in name.split('/') {
            node = node.children.entry(part.to_string()).or_default();
        }
        node.leaf = Some((index, *time));
    }
    children(&root, "", sort)
}

fn children(trie: &Trie, prefix: &str, sort: Sort) -> Vec<Node> {
    let mut nodes = Vec::new();
    for (segment, child) in &trie.children {
        let path = if prefix.is_empty() {
            segment.clone()
        } else {
            format!("{prefix}/{segment}")
        };
        if let Some((index, time)) = child.leaf {
            nodes.push(Node::Leaf(Leaf {
                index,
                label: segment.clone(),
                time,
            }));
        }
        if child.children.is_empty() {
            continue;
        }
        let mut inner = children(child, &path, sort);
        let node = if inner.len() == 1 {
            match inner.remove(0) {
                Node::Leaf(mut leaf) => {
                    leaf.label = format!("{segment}/{}", leaf.label);
                    Node::Leaf(leaf)
                }
                Node::Folder(mut folder) => {
                    folder.label = format!("{segment}/{}", folder.label);
                    Node::Folder(folder)
                }
            }
        } else {
            let count = inner.iter().map(|n| n.leaves().len()).sum();
            let newest = inner.iter().map(Node::newest).max().unwrap_or(0);
            Node::Folder(Folder {
                path,
                label: segment.clone(),
                children: inner,
                count,
                newest,
            })
        };
        nodes.push(node);
    }
    order(&mut nodes, sort);
    nodes
}

fn order(nodes: &mut [Node], sort: Sort) {
    nodes.sort_by(|a, b| {
        let folder_first = matches!(b, Node::Folder(_)).cmp(&matches!(a, Node::Folder(_)));
        folder_first.then_with(|| match sort {
            Sort::Name => natural_key(a.label()).cmp(&natural_key(b.label())),
            Sort::Newest => b
                .newest()
                .cmp(&a.newest())
                .then_with(|| natural_key(a.label()).cmp(&natural_key(b.label()))),
        })
    });
}

fn natural_key(label: &str) -> String {
    label.to_lowercase()
}

/// Case-insensitive match of `query` in `text`: a contiguous substring when
/// there is one, otherwise the query's characters in order. Returns the
/// matched byte ranges of `text`, merged where they touch.
pub fn fuzzy(text: &str, query: &str) -> Option<Vec<Range<usize>>> {
    let query = query.trim();
    if query.is_empty() {
        return Some(Vec::new());
    }
    let lower_text: Vec<(usize, char)> = text
        .char_indices()
        .map(|(i, c)| (i, c.to_ascii_lowercase()))
        .collect();
    let lower_query: Vec<char> = query.chars().map(|c| c.to_ascii_lowercase()).collect();
    let end_of = |k: usize| lower_text.get(k + 1).map_or(text.len(), |(i, _)| *i);
    if let Some(start) = (0..lower_text.len()).find(|&s| {
        lower_query
            .iter()
            .enumerate()
            .all(|(q, c)| lower_text.get(s + q).is_some_and(|(_, t)| t == c))
    }) {
        let whole = lower_text[start].0..end_of(start + lower_query.len() - 1);
        return Some(std::iter::once(whole).collect());
    }
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut wanted = lower_query.iter().peekable();
    for (k, (at, c)) in lower_text.iter().enumerate() {
        if wanted.peek() == Some(&c) {
            wanted.next();
            let end = end_of(k);
            match ranges.last_mut() {
                Some(last) if last.end == *at => last.end = end,
                _ => ranges.push(*at..end),
            }
        }
    }
    wanted.peek().is_none().then_some(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(nodes: &[Node]) -> Vec<String> {
        nodes
            .iter()
            .map(|n| match n {
                Node::Folder(f) => format!("{}/ ({})", f.label, f.count),
                Node::Leaf(l) => l.label.clone(),
            })
            .collect()
    }

    #[test]
    fn nests_by_slash_and_counts() {
        let tree = build(
            &[
                ("main", 0),
                ("feat/ui/a", 0),
                ("feat/ui/b", 0),
                ("feat/api", 0),
                ("fix/x", 0),
                ("fix/y", 0),
            ],
            Sort::Name,
        );
        assert_eq!(labels(&tree), ["feat/ (3)", "fix/ (2)", "main"]);
        let Node::Folder(feat) = &tree[0] else {
            panic!("feat is a folder")
        };
        assert_eq!(labels(&feat.children), ["ui/ (2)", "api"]);
        assert_eq!(feat.path, "feat");
    }

    #[test]
    fn single_children_fold_into_their_parent() {
        let tree = build(
            &[("feat/solo", 0), ("chore/deps/a", 0), ("chore/deps/b", 0)],
            Sort::Name,
        );
        assert_eq!(labels(&tree), ["chore/deps/ (2)", "feat/solo"]);
        let Node::Folder(deps) = &tree[0] else {
            panic!("chore/deps is a folder")
        };
        assert_eq!(deps.path, "chore/deps");
    }

    #[test]
    fn remotes_group_by_remote_then_prefix() {
        let tree = build(
            &[
                ("origin/main", 0),
                ("origin/feat/a", 0),
                ("origin/feat/b", 0),
                ("fork/main", 0),
            ],
            Sort::Name,
        );
        assert_eq!(labels(&tree), ["origin/ (3)", "fork/main"]);
        let Node::Folder(origin) = &tree[0] else {
            panic!("origin is a folder")
        };
        assert_eq!(labels(&origin.children), ["feat/ (2)", "main"]);
    }

    #[test]
    fn newest_sort_orders_folders_and_leaves_by_latest_commit() {
        let tree = build(
            &[
                ("a", 1),
                ("b", 5),
                ("old/x", 2),
                ("old/y", 3),
                ("new/x", 9),
                ("new/y", 1),
            ],
            Sort::Newest,
        );
        assert_eq!(labels(&tree), ["new/ (2)", "old/ (2)", "b", "a"]);
    }

    #[test]
    fn leaf_indices_point_back_to_the_input() {
        let tree = build(&[("z/a", 0), ("z/b", 0)], Sort::Name);
        assert_eq!(tree[0].leaves(), [0, 1]);
    }

    fn spans(pairs: &[(usize, usize)]) -> Option<Vec<Range<usize>>> {
        Some(pairs.iter().map(|&(a, b)| a..b).collect())
    }

    #[test]
    fn fuzzy_prefers_substrings_then_subsequences() {
        assert_eq!(fuzzy("feat/delighters", "light"), spans(&[(7, 12)]));
        assert_eq!(fuzzy("feat/Delighters", "DEL"), spans(&[(5, 8)]));
        assert_eq!(
            fuzzy("fix/boot-loader", "fbl"),
            Some(vec![0..1, 4..5, 9..10])
        );
        assert_eq!(fuzzy("fix/boot-loader", "fbz"), None);
        assert_eq!(fuzzy("anything", " "), Some(Vec::new()));
    }

    #[test]
    fn fuzzy_ranges_are_byte_ranges_on_unicode() {
        let text = "café/ü-x";
        let ranges = fuzzy(text, "ü").unwrap();
        assert_eq!(&text[ranges[0].clone()], "ü");
    }
}
