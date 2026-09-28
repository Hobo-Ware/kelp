use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const SPEC: &str = "version https://git-lfs.github.com/spec/v1";
const MAX_POINTER_BYTES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    pub oid: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sides {
    pub old: Option<Pointer>,
    pub new: Option<Pointer>,
}

impl Sides {
    pub fn is_empty(&self) -> bool {
        self.old.is_none() && self.new.is_none()
    }

    pub fn shown(&self) -> Option<&Pointer> {
        self.new.as_ref().or(self.old.as_ref())
    }
}

pub fn parse(bytes: &[u8]) -> Option<Pointer> {
    if bytes.len() > MAX_POINTER_BYTES || !bytes.starts_with(SPEC.as_bytes()) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let mut oid = None;
    let mut size = None;
    for line in text.lines().skip(1) {
        if let Some(hash) = line.strip_prefix("oid sha256:") {
            let hash = hash.trim();
            if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
                oid = Some(hash.to_ascii_lowercase());
            }
        } else if let Some(n) = line.strip_prefix("size ") {
            size = n.trim().parse().ok();
        }
    }
    Some(Pointer {
        oid: oid?,
        size: size?,
    })
}

pub fn object_path(common_dir: &Path, oid: &str) -> PathBuf {
    common_dir
        .join("lfs")
        .join("objects")
        .join(&oid[..2])
        .join(&oid[2..4])
        .join(oid)
}

pub fn resolve(common_dir: &Path, bytes: Option<Vec<u8>>) -> (Option<Vec<u8>>, Option<Pointer>) {
    let Some(pointer) = bytes.as_deref().and_then(parse) else {
        return (bytes, None);
    };
    match std::fs::read(object_path(common_dir, &pointer.oid)) {
        Ok(content) if content.len() as u64 == pointer.size => (Some(content), Some(pointer)),
        _ => (bytes, Some(pointer)),
    }
}

pub fn installed() -> bool {
    static FOUND: OnceLock<bool> = OnceLock::new();
    *FOUND.get_or_init(|| {
        std::env::var_os("PATH").is_some_and(|path| {
            std::env::split_paths(&path).any(|dir| dir.join("git-lfs").is_file())
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OID: &str = "4d7a214614ab2935c943f9e0ff69d22eadbb8f32b1258daaa5e2ca24d17e2393";

    fn pointer(oid: &str, size: u64) -> Vec<u8> {
        format!("{SPEC}\noid sha256:{oid}\nsize {size}\n").into_bytes()
    }

    #[test]
    fn pointers_parse() {
        assert_eq!(
            parse(&pointer(OID, 12345)),
            Some(Pointer {
                oid: OID.into(),
                size: 12345
            })
        );
    }

    #[test]
    fn other_files_are_not_pointers() {
        assert_eq!(parse(b"hello"), None);
        assert_eq!(parse(format!("{SPEC}\nsize 3\n").as_bytes()), None);
        assert_eq!(parse(&pointer("nothex", 3)), None);
        let mut big = pointer(OID, 3);
        big.resize(MAX_POINTER_BYTES + 1, b' ');
        assert_eq!(parse(&big), None);
    }

    #[test]
    fn local_objects_replace_the_pointer() {
        let dir = std::env::temp_dir().join(format!("kelp-lfs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let object = object_path(&dir, OID);
        std::fs::create_dir_all(object.parent().unwrap()).unwrap();
        std::fs::write(&object, b"real bytes").unwrap();

        let (content, found) = resolve(&dir, Some(pointer(OID, 10)));
        assert_eq!(content.as_deref(), Some(&b"real bytes"[..]));
        assert_eq!(found.map(|p| p.size), Some(10));

        let other = "a".repeat(64);
        let (content, found) = resolve(&dir, Some(pointer(&other, 10)));
        assert_eq!(content, Some(pointer(&other, 10)));
        assert!(found.is_some());

        let (content, found) = resolve(&dir, Some(b"plain".to_vec()));
        assert_eq!(content.as_deref(), Some(&b"plain"[..]));
        assert!(found.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
