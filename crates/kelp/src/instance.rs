use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use eframe::egui;

const REPLY_TIMEOUT: Duration = Duration::from_secs(2);
const READY: &str = "ok";

pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("KELP_INSTANCE_SOCKET") {
        return path.into();
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let base = if cfg!(target_os = "macos") {
        home.join("Library/Caches")
    } else {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"))
    };
    base.join("kelp").join("instance.sock")
}

pub fn hand_off(socket: &Path, paths: &[PathBuf]) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(REPLY_TIMEOUT))?;
    stream.set_write_timeout(Some(REPLY_TIMEOUT))?;
    let message = serde_json::to_string(paths).map_err(std::io::Error::other)?;
    writeln!(stream, "{message}")?;
    let mut reply = String::new();
    BufReader::new(&stream).read_line(&mut reply)?;
    if reply.trim() == READY {
        Ok(())
    } else {
        Err(std::io::Error::other("the running Kelp did not answer"))
    }
}

pub struct Listener {
    requests: Receiver<Vec<PathBuf>>,
    socket: PathBuf,
}

impl Listener {
    pub fn start(socket: &Path, ctx: egui::Context) -> Option<Self> {
        if UnixStream::connect(socket).is_ok() {
            return None;
        }
        let _ = std::fs::remove_file(socket);
        if let Some(dir) = socket.parent() {
            std::fs::create_dir_all(dir).ok()?;
        }
        let listener = UnixListener::bind(socket)
            .inspect_err(|e| eprintln!("kelp: not listening on {}: {e}", socket.display()))
            .ok()?;
        restrict_to_owner(socket);
        let (tx, requests) = mpsc::channel();
        std::thread::Builder::new()
            .name("kelp-instance".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    if let Some(paths) = read_request(&stream) {
                        if tx.send(paths).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                        let _ = writeln!(&stream, "{READY}");
                    }
                }
            })
            .ok()?;
        Some(Self {
            requests,
            socket: socket.to_path_buf(),
        })
    }

    pub fn take(&self) -> Vec<Vec<PathBuf>> {
        self.requests.try_iter().collect()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn read_request(stream: &UnixStream) -> Option<Vec<PathBuf>> {
    stream.set_read_timeout(Some(REPLY_TIMEOUT)).ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str(line.trim()).ok()
}

fn restrict_to_owner(socket: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_socket(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("kelp-{name}-{}.sock", std::process::id()))
    }

    fn wait_for(listener: &Listener) -> Vec<Vec<PathBuf>> {
        for _ in 0..100 {
            let got = listener.take();
            if !got.is_empty() {
                return got;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Vec::new()
    }

    #[test]
    fn paths_reach_the_running_instance() {
        let socket = temp_socket("handoff");
        let listener = Listener::start(&socket, egui::Context::default()).unwrap();
        let paths = vec![PathBuf::from("/tmp/a repo"), PathBuf::from("/tmp/b")];
        hand_off(&socket, &paths).unwrap();
        assert_eq!(wait_for(&listener), vec![paths]);
        let _ = std::fs::remove_file(&socket);
    }

    #[test]
    fn a_second_instance_does_not_take_over_the_socket() {
        let socket = temp_socket("second");
        let first = Listener::start(&socket, egui::Context::default()).unwrap();
        assert!(Listener::start(&socket, egui::Context::default()).is_none());
        hand_off(&socket, &[PathBuf::from("/x")]).unwrap();
        assert_eq!(wait_for(&first).len(), 1);
        let _ = std::fs::remove_file(&socket);
    }

    #[test]
    fn a_stale_socket_file_is_replaced() {
        let socket = temp_socket("stale");
        std::fs::write(&socket, b"").unwrap();
        assert!(hand_off(&socket, &[]).is_err());
        let listener = Listener::start(&socket, egui::Context::default()).unwrap();
        hand_off(&socket, &[]).unwrap();
        assert_eq!(wait_for(&listener), vec![Vec::<PathBuf>::new()]);
        let _ = std::fs::remove_file(&socket);
    }
}
