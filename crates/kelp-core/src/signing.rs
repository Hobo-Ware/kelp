use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git_cli;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signature {
    Unsigned,
    Verified { signer: String, key: String },
    UnknownKey { signer: String, key: String },
    Expired { signer: String, key: String },
    Unverified { signer: String, key: String },
}

impl Signature {
    pub fn parse(out: &str) -> Self {
        let mut fields = out.trim_end_matches('\n').split('\0');
        let code = fields.next().unwrap_or("N").trim();
        let signer = fields.next().unwrap_or_default().trim().to_string();
        let key = fields.next().unwrap_or_default().trim().to_string();
        match code {
            "G" => Signature::Verified { signer, key },
            "U" | "E" => Signature::UnknownKey { signer, key },
            "X" | "Y" => Signature::Expired { signer, key },
            "B" | "R" => Signature::Unverified { signer, key },
            _ => Signature::Unsigned,
        }
    }

    pub fn label(&self) -> Option<&'static str> {
        match self {
            Signature::Unsigned => None,
            Signature::Verified { .. } => Some("Verified"),
            Signature::UnknownKey { .. } => Some("Signed (unknown key)"),
            Signature::Expired { .. } => Some("Signed (expired key)"),
            Signature::Unverified { .. } => Some("Unverified"),
        }
    }

    pub fn detail(&self) -> Option<String> {
        let (signer, key) = match self {
            Signature::Unsigned => return None,
            Signature::Verified { signer, key }
            | Signature::UnknownKey { signer, key }
            | Signature::Expired { signer, key }
            | Signature::Unverified { signer, key } => (signer, key),
        };
        let signer = if signer.is_empty() {
            "unknown signer"
        } else {
            signer
        };
        Some(if key.is_empty() {
            signer.to_string()
        } else {
            format!("{signer} · key {key}")
        })
    }
}

pub fn read(dir: &Path, commit: &str) -> anyhow::Result<Signature> {
    let out = git_cli::run(dir, &["log", "-1", "--format=%G?%x00%GS%x00%GK", commit])?;
    Ok(Signature::parse(&out))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Gpg,
    Ssh,
}

impl Format {
    fn config_value(self) -> &'static str {
        match self {
            Format::Gpg => "openpgp",
            Format::Ssh => "ssh",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Repo,
    Global,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub sign: bool,
    pub format: Format,
    pub key: String,
}

pub fn current(dir: &Path) -> Config {
    let get = |key: &str| {
        git_cli::run(dir, &["config", "--get", key])
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    };
    Config {
        sign: get("commit.gpgsign") == "true",
        format: if get("gpg.format") == "ssh" {
            Format::Ssh
        } else {
            Format::Gpg
        },
        key: get("user.signingkey"),
    }
}

pub fn apply(dir: &Path, scope: Scope, config: &Config) -> anyhow::Result<()> {
    let set = |key: &str, value: &str| {
        let mut args = vec!["config"];
        if scope == Scope::Global {
            args.push("--global");
        }
        args.extend([key, value]);
        git_cli::run(dir, &args).map(|_| ())
    };
    set("commit.gpgsign", if config.sign { "true" } else { "false" })?;
    set("gpg.format", config.format.config_value())?;
    if config.key.trim().is_empty() {
        let mut args = vec!["config"];
        if scope == Scope::Global {
            args.push("--global");
        }
        args.extend(["--unset", "user.signingkey"]);
        let _ = git_cli::run(dir, &args);
    } else {
        set("user.signingkey", config.key.trim())?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub format: Format,
    pub id: String,
    pub label: String,
}

pub fn detect_keys(home: &Path) -> Vec<Key> {
    let mut keys = ssh_keys(&home.join(".ssh"));
    keys.extend(gpg_keys());
    keys
}

fn ssh_keys(dir: &Path) -> Vec<Key> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "pub"))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let comment = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| text.split_whitespace().nth(2).map(str::to_string));
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            Key {
                format: Format::Ssh,
                label: comment.map_or(name.clone(), |c| format!("{name} ({c})")),
                id: path.to_string_lossy().into_owned(),
            }
        })
        .collect()
}

fn gpg_keys() -> Vec<Key> {
    let Ok(output) = Command::new("gpg")
        .args(["--list-secret-keys", "--with-colons"])
        .output()
    else {
        return Vec::new();
    };
    parse_gpg(&String::from_utf8_lossy(&output.stdout))
}

pub fn parse_gpg(out: &str) -> Vec<Key> {
    let mut keys: Vec<Key> = Vec::new();
    let mut pending: Option<String> = None;
    for line in out.lines() {
        let fields: Vec<&str> = line.split(':').collect();
        match fields.first() {
            Some(&"sec") => pending = fields.get(4).map(|id| id.to_string()),
            Some(&"uid") => {
                if let Some(id) = pending.take() {
                    let uid = fields.get(9).copied().unwrap_or_default();
                    keys.push(Key {
                        format: Format::Gpg,
                        label: format!("{uid} ({id})"),
                        id,
                    });
                }
            }
            _ => {}
        }
    }
    keys
}

pub fn explain_failure(stderr: &str) -> Option<&'static str> {
    let text = stderr.to_ascii_lowercase();
    if text.contains("gpg failed to sign") || text.contains("inappropriate ioctl for device") {
        Some(
            "Git could not sign the commit with GPG. Check that gpg-agent and a pinentry app (like pinentry-mac) are installed, or turn signing off in Settings.",
        )
    } else if text.contains("incorrect passphrase")
        || text.contains("couldn't get agent socket")
        || text.contains("agent refused operation")
    {
        Some(
            "Git could not sign the commit with your SSH key. Add the key to ssh-agent (ssh-add) so Kelp can use it without a prompt.",
        )
    } else if text.contains("no secret key")
        || text.contains("no such file or directory") && text.contains("signing")
    {
        Some("The signing key in your git config was not found. Pick another key in Settings.")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_signature_code_maps() {
        let with = |code: &str| Signature::parse(&format!("{code}\0Maya\0ABC123\n"));
        assert!(matches!(with("G"), Signature::Verified { .. }));
        assert!(matches!(with("U"), Signature::UnknownKey { .. }));
        assert!(matches!(with("E"), Signature::UnknownKey { .. }));
        assert!(matches!(with("X"), Signature::Expired { .. }));
        assert!(matches!(with("Y"), Signature::Expired { .. }));
        assert!(matches!(with("B"), Signature::Unverified { .. }));
        assert!(matches!(with("R"), Signature::Unverified { .. }));
        assert_eq!(with("N"), Signature::Unsigned);
        assert_eq!(Signature::parse(""), Signature::Unsigned);
        assert_eq!(with("G").detail().as_deref(), Some("Maya · key ABC123"));
        assert_eq!(with("N").label(), None);
    }

    #[test]
    fn gpg_listing_parses() {
        let out = "sec:u:4096:1:ABCDEF0123456789:1600000000:::u:::scESC:::+:::23::0:\n\
                   fpr:::::::::0123:\n\
                   uid:u::::1600000000::HASH::Maya Lindqvist <maya@example.com>::::::::::0:\n";
        let keys = parse_gpg(out);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].id, "ABCDEF0123456789");
        assert!(keys[0].label.starts_with("Maya Lindqvist"));
    }

    #[test]
    fn signing_failures_are_explained() {
        assert!(
            explain_failure(
                "error: gpg failed to sign the data\nfatal: failed to write commit object"
            )
            .is_some()
        );
        assert!(explain_failure("Couldn't get agent socket?").is_some());
        assert!(explain_failure("nothing to commit").is_none());
    }

    #[test]
    fn ssh_keys_are_found() {
        let dir = std::env::temp_dir().join(format!("kelp-ssh-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".ssh")).unwrap();
        std::fs::write(
            dir.join(".ssh/id_ed25519.pub"),
            "ssh-ed25519 AAAA maya@laptop\n",
        )
        .unwrap();
        std::fs::write(dir.join(".ssh/id_ed25519"), "secret").unwrap();
        let keys = ssh_keys(&dir.join(".ssh"));
        assert_eq!(keys.len(), 1);
        assert!(keys[0].id.ends_with("id_ed25519.pub"));
        assert_eq!(keys[0].label, "id_ed25519.pub (maya@laptop)");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
