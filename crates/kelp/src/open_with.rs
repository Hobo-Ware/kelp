use std::path::{Path, PathBuf};
use std::process::Command;

const KNOWN_EDITORS: [&str; 4] = ["Cursor", "Visual Studio Code", "Zed", "Sublime Text"];

pub fn editor_app(setting: &str) -> Option<String> {
    let chosen = setting.trim();
    if !chosen.is_empty() {
        return Some(chosen.to_string());
    }
    KNOWN_EDITORS
        .iter()
        .find(|name| installed(name))
        .map(|name| name.to_string())
}

fn installed(app: &str) -> bool {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    [PathBuf::from("/Applications"), home.join("Applications")]
        .iter()
        .any(|dir| dir.join(format!("{app}.app")).exists())
}

fn editor_command(path: &Path, app: Option<&str>) -> Command {
    if cfg!(target_os = "macos") {
        let mut c = Command::new("open");
        if let Some(app) = app {
            c.args(["-a", app]);
        }
        c.arg(path);
        c
    } else {
        let mut c = Command::new(app.unwrap_or("xdg-open"));
        c.arg(path);
        c
    }
}

pub fn open_in_editor(path: &Path, app: Option<&str>) -> std::io::Result<()> {
    let mut command = editor_command(path, app);
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(match app {
            Some(app) => format!("{app} could not open it"),
            None => "no app is set to open this file".to_string(),
        }))
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn the_system_opener_is_the_default() {
        let command = editor_command(Path::new("/tmp/a.txt"), None);
        assert_eq!(command.get_program(), "xdg-open");
    }

    #[test]
    fn a_chosen_editor_runs_as_a_command() {
        let command = editor_command(Path::new("/tmp/a.txt"), Some("code"));
        assert_eq!(command.get_program(), "code");
        assert_eq!(command.get_args().collect::<Vec<_>>(), ["/tmp/a.txt"]);
    }
}
