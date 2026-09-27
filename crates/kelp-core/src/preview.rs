use std::sync::Arc;

const MAX_PREVIEW_BYTES: usize = 32 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Svg,
    Markdown,
}

impl Kind {
    pub fn of(path: &str) -> Option<Self> {
        let ext = path.rsplit_once('.')?.1.to_ascii_lowercase();
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "tif" | "tiff" => {
                Some(Kind::Image)
            }
            "svg" => Some(Kind::Svg),
            "md" | "markdown" | "mdown" | "mkd" => Some(Kind::Markdown),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sides {
    pub kind: Kind,
    pub old: Option<Arc<[u8]>>,
    pub new: Option<Arc<[u8]>>,
}

impl Sides {
    pub fn capture(path: &str, old: Option<&[u8]>, new: Option<&[u8]>) -> Option<Self> {
        let kind = Kind::of(path)?;
        let keep = |b: Option<&[u8]>| {
            b.filter(|b| !b.is_empty() && b.len() <= MAX_PREVIEW_BYTES)
                .map(Arc::from)
        };
        Some(Self {
            kind,
            old: keep(old),
            new: keep(new),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_by_extension() {
        assert_eq!(Kind::of("assets/Icon.PNG"), Some(Kind::Image));
        assert_eq!(Kind::of("a/b.jpeg"), Some(Kind::Image));
        assert_eq!(Kind::of("site/mascot.svg"), Some(Kind::Svg));
        assert_eq!(Kind::of("README.md"), Some(Kind::Markdown));
        assert_eq!(Kind::of("src/main.rs"), None);
        assert_eq!(Kind::of("Makefile"), None);
    }

    #[test]
    fn empty_sides_are_dropped() {
        let sides = Sides::capture("a.png", None, Some(b"x")).unwrap();
        assert!(sides.old.is_none());
        assert_eq!(sides.new.as_deref(), Some(&b"x"[..]));
        assert!(Sides::capture("a.rs", None, Some(b"x")).is_none());
    }
}
