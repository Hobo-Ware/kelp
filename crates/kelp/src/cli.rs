use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const USAGE: &str = "Usage: kelp [options] [folder ...]

Opens each folder as a tab. From a terminal Kelp starts in the background
and returns the prompt; if Kelp is already running, the folders open there.

Options:
  -w, --wait     Stay in the foreground until Kelp quits
  -h, --help     Show this help
  -V, --version  Show the version";

#[derive(Debug, PartialEq, Eq)]
pub struct Args {
    pub paths: Vec<PathBuf>,
    pub wait: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Parsed {
    Run(Args),
    Print(String),
    Fail(String),
}

pub fn parse(raw: impl IntoIterator<Item = String>) -> Parsed {
    let mut args = Args {
        paths: Vec::new(),
        wait: false,
    };
    let mut only_paths = false;
    for arg in raw {
        match arg.as_str() {
            _ if only_paths => args.paths.push(arg.into()),
            "--" => only_paths = true,
            "-w" | "--wait" => args.wait = true,
            "-h" | "--help" => return Parsed::Print(USAGE.to_string()),
            "-V" | "--version" => {
                return Parsed::Print(format!("kelp {}", env!("CARGO_PKG_VERSION")));
            }
            flag if flag.starts_with('-') && flag.len() > 1 => {
                return Parsed::Fail(format!("kelp: unknown option {flag}\n\n{USAGE}"));
            }
            _ => args.paths.push(arg.into()),
        }
    }
    Parsed::Run(args)
}

pub fn resolve(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    paths
        .iter()
        .map(|p| std::fs::canonicalize(p).map_err(|e| format!("kelp: {}: {e}", p.display())))
        .collect()
}

pub enum Start {
    Here(Vec<PathBuf>),
    Done,
}

pub fn start() -> Result<Start, String> {
    let args = match parse(std::env::args().skip(1)) {
        Parsed::Run(args) => args,
        Parsed::Print(text) => {
            println!("{text}");
            return Ok(Start::Done);
        }
        Parsed::Fail(error) => return Err(error),
    };
    let paths = resolve(&args.paths)?;
    let from_terminal = std::io::stdout().is_terminal() || std::io::stderr().is_terminal();
    if args.wait || !from_terminal || crate::settings::is_dev_run() {
        return Ok(Start::Here(paths));
    }
    let handoff = if paths.is_empty() {
        current_repo().into_iter().collect()
    } else {
        paths.clone()
    };
    if crate::instance::hand_off(&crate::instance::socket_path(), &handoff).is_ok() {
        return Ok(Start::Done);
    }
    match launch_detached(&paths) {
        Ok(()) => Ok(Start::Done),
        Err(_) => Ok(Start::Here(paths)),
    }
}

fn current_repo() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .filter(|dir| gix::discover(dir).is_ok())
}

fn launch_detached(paths: &[PathBuf]) -> std::io::Result<()> {
    let exe = std::env::current_exe()?.canonicalize()?;
    let mut command = match app_bundle(&exe) {
        Some(bundle) => {
            let mut c = Command::new("open");
            c.arg("-n").arg("-a").arg(bundle);
            if !paths.is_empty() {
                c.arg("--args").args(paths);
            }
            c
        }
        None => {
            use std::os::unix::process::CommandExt;
            let mut c = Command::new(exe);
            c.args(paths).process_group(0);
            c
        }
    };
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

pub fn app_bundle(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|dir| dir.extension().is_some_and(|ext| ext == "app"))
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Parsed {
        parse(list.iter().map(|s| s.to_string()))
    }

    #[test]
    fn folders_and_wait() {
        assert_eq!(
            args(&["-w", "a", "b"]),
            Parsed::Run(Args {
                paths: vec!["a".into(), "b".into()],
                wait: true,
            })
        );
        assert_eq!(
            args(&[]),
            Parsed::Run(Args {
                paths: vec![],
                wait: false,
            })
        );
    }

    #[test]
    fn help_version_and_unknown_flags() {
        assert!(matches!(args(&["--help"]), Parsed::Print(t) if t.starts_with("Usage")));
        assert!(matches!(args(&["-V"]), Parsed::Print(t) if t.starts_with("kelp ")));
        assert!(matches!(args(&["--nope"]), Parsed::Fail(t) if t.contains("--nope")));
    }

    #[test]
    fn a_double_dash_ends_options() {
        assert_eq!(
            args(&["--", "-w"]),
            Parsed::Run(Args {
                paths: vec!["-w".into()],
                wait: false,
            })
        );
    }

    #[test]
    fn missing_folders_are_reported() {
        let error = resolve(&["/definitely/not/here".into()]).unwrap_err();
        assert!(error.contains("/definitely/not/here"));
    }

    #[test]
    fn the_bundle_is_found_from_the_binary() {
        assert_eq!(
            app_bundle(Path::new("/Applications/Kelp.app/Contents/MacOS/kelp")),
            Some(PathBuf::from("/Applications/Kelp.app"))
        );
        assert_eq!(
            app_bundle(Path::new("/Users/me/kelp/target/release/kelp")),
            None
        );
    }
}
