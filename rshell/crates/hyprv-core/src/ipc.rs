use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    future::Future,
    io,
    os::{
        fd::AsRawFd,
        unix::{
            fs::{FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::UnixStream as StdStream,
        },
    },
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::{oneshot, watch},
    task::JoinSet,
};
const MAX_FRAME: u64 = 65536;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub target: String,
    pub function: String,
    #[serde(default)]
    pub args: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    pub result: String,
}
impl Response {
    pub fn success(result: impl Into<String>) -> Self {
        Self {
            ok: true,
            result: result.into(),
        }
    }
    pub fn error(result: impl Into<String>) -> Self {
        Self {
            ok: false,
            result: result.into(),
        }
    }
}
impl Request {
    pub fn validate(&self) -> Result<(), &'static str> {
        let count = match (self.target.as_str(), self.function.as_str()) {
            ("bar", "toggle" | "layoutStatus") => 1,
            ("bar", "inputStatus" | "dataStatus") | ("controlPanel" | "calendar", "toggle") => 0,
            (
                "quickAdjust",
                "showBrightness"
                | "showBrightnessIncrease"
                | "showBrightnessDecrease"
                | "showVolume",
            ) => 0,
            ("quickAdjust", "showBrightnessLevel") => 1,
            ("shell", "kill") => 0,
            // hypr/scripts after cutover: toggle-battery-mode.sh, rotate-screen-ccw.sh.
            ("desktop", "setVariant") => 1,
            ("desktop", "deskflowReflow") => 0,
            _ => return Err("unknown IPC target/function"),
        };
        if self.args.len() != count {
            return Err("incorrect argument count");
        }
        if self.target == "quickAdjust" && count == 1 && self.args[0].parse::<i32>().is_err() {
            return Err("brightness level must be an integer");
        }
        if self.function == "setVariant" && !matches!(self.args[0].as_str(), "v1" | "v2" | "v3" | "bt") {
            return Err("variant must be v1, v2, v3 or bt");
        }
        Ok(())
    }
}
pub fn socket_path() -> io::Result<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(|p| PathBuf::from(p).join("hyprv/shell.sock"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "XDG_RUNTIME_DIR missing"))
}
/// Holds flock for the instance lifetime. Never unlinks the lock inode.
pub struct InstanceLock {
    _file: File,
}
impl InstanceLock {
    pub fn acquire(socket: &Path) -> io::Result<Self> {
        let parent = socket
            .parent()
            .ok_or_else(|| io::Error::other("socket has no parent"))?;
        match std::fs::DirBuilder::new().mode(0o700).create(parent) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e),
        }
        let metadata = std::fs::symlink_metadata(parent)?;
        if !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.mode() & 0o077 != 0
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "IPC directory must be owned by current user and mode 0700",
            ));
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(parent.join("shell.lock"))?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "hyprv-shell already running",
            ));
        }
        Ok(Self { _file: file })
    }
}
use std::os::unix::fs::DirBuilderExt;
pub struct IpcServer {
    listener: UnixListener,
    socket: PathBuf,
    /// (st_dev, st_ino) of the socket this server bound; only that inode is unlinked.
    identity: (u64, u64),
    _lock: InstanceLock,
}
pub type Handler =
    Arc<dyn Fn(Request) -> Pin<Box<dyn Future<Output = Response> + Send>> + Send + Sync>;
impl IpcServer {
    pub fn bind(socket: PathBuf) -> io::Result<Self> {
        let lock = InstanceLock::acquire(&socket)?;
        match std::fs::symlink_metadata(&socket) {
            Ok(meta) if meta.file_type().is_socket() => {
                if StdStream::connect(&socket).is_ok() {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        "live IPC socket exists",
                    ));
                }
                std::fs::remove_file(&socket)?;
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "refusing to unlink non-socket",
                ))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        let listener = UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let meta = std::fs::symlink_metadata(&socket)?;
        Ok(Self {
            listener,
            identity: (meta.dev(), meta.ino()),
            socket,
            _lock: lock,
        })
    }
    pub async fn run(
        &self,
        handler: Handler,
        mut shutdown: watch::Receiver<bool>,
    ) -> io::Result<()> {
        let mut clients = JoinSet::new();
        loop {
            if *shutdown.borrow() {
                break;
            }
            tokio::select! {
                accepted = self.listener.accept(), if clients.len() < 64 => {
                    let (stream, _) = accepted?;
                    if stream.peer_cred()?.uid() != unsafe { libc::geteuid() } { continue; }
                    let handler = handler.clone();
                    clients.spawn(async move { let _ = tokio::time::timeout(Duration::from_secs(10), serve(stream, handler)).await; });
                },
                _ = clients.join_next(), if !clients.is_empty() => (),
                _ = shutdown.changed() => break,
            }
        }
        // Let in-flight replies (including kill acknowledgement) flush first.
        let _ = tokio::time::timeout(Duration::from_secs(1), async {
            while clients.join_next().await.is_some() {}
        })
        .await;
        clients.abort_all();
        while clients.join_next().await.is_some() {}
        Ok(())
    }
}
impl Drop for IpcServer {
    /// CORE-2: never unlink a replacement (regular file, symlink or another socket).
    /// Runs while the instance lock is still held (fields drop after this).
    fn drop(&mut self) {
        if let Ok(meta) = std::fs::symlink_metadata(&self.socket) {
            if meta.file_type().is_socket() && (meta.dev(), meta.ino()) == self.identity {
                let _ = std::fs::remove_file(&self.socket);
            }
        }
    }
}
async fn frame(reader: impl AsyncRead + Unpin) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    BufReader::new(reader.take(MAX_FRAME + 1))
        .read_until(b'\n', &mut bytes)
        .await?;
    if bytes.len() as u64 > MAX_FRAME || bytes.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC frame too large or unterminated",
        ));
    }
    Ok(bytes)
}
async fn serve(mut stream: UnixStream, handler: Handler) -> io::Result<()> {
    let response = match frame(&mut stream).await {
        Ok(bytes) => match serde_json::from_slice::<Request>(&bytes) {
            Ok(request) => match request.validate() {
                Ok(()) => handler(request).await,
                Err(e) => Response::error(e),
            },
            Err(e) => Response::error(e.to_string()),
        },
        Err(e) => Response::error(e.to_string()),
    };
    let mut bytes = serde_json::to_vec(&response)?;
    bytes.push(b'\n');
    stream.write_all(&bytes).await
}
pub async fn call(socket: &Path, request: &Request) -> io::Result<Response> {
    request
        .validate()
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut stream = UnixStream::connect(socket).await?;
        let mut bytes = serde_json::to_vec(request)?;
        bytes.push(b'\n');
        if bytes.len() as u64 > MAX_FRAME {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "IPC request too large",
            ));
        }
        stream.write_all(&bytes).await?;
        serde_json::from_slice(&frame(&mut stream).await?).map_err(io::Error::other)
    })
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "IPC timeout"))?
}
pub struct UiRequest {
    pub request: Request,
    pub reply: oneshot::Sender<Response>,
}
pub fn ui_handler(sender: calloop::channel::Sender<UiRequest>) -> Handler {
    Arc::new(move |request| {
        let (reply, receive) = oneshot::channel();
        let sent = sender.send(UiRequest { request, reply }).is_ok();
        Box::pin(async move {
            if sent {
                receive
                    .await
                    .unwrap_or_else(|_| Response::error("UI disconnected"))
            } else {
                Response::error("UI disconnected")
            }
        })
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hyprv-ipc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[tokio::test(flavor = "current_thread")]
    async fn drop_preserves_replacement_file() {
        let dir = scratch("replace");
        let socket = dir.join("ipc").join("s.sock");
        let server = IpcServer::bind(socket.clone()).unwrap();
        std::fs::remove_file(&socket).unwrap();
        std::fs::write(&socket, "replacement data").unwrap();
        drop(server);
        assert_eq!(std::fs::read_to_string(&socket).ok().as_deref(), Some("replacement data"));
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn drop_removes_own_socket() {
        let dir = scratch("own");
        let socket = dir.join("ipc").join("s.sock");
        drop(IpcServer::bind(socket.clone()).unwrap());
        assert!(std::fs::symlink_metadata(&socket).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
    fn path(label: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("hyprv-ipc-test-{}-{label}", std::process::id()))
            .join("shell.sock")
    }
    #[test]
    fn contract_validation() {
        for (target, function, args) in [
            ("bar", "toggle", vec!["DP-4"]),
            ("bar", "dataStatus", vec![]),
            ("quickAdjust", "showBrightnessLevel", vec!["42"]),
            ("calendar", "toggle", vec![]),
        ] {
            assert!(Request {
                target: target.into(),
                function: function.into(),
                args: args.into_iter().map(String::from).collect()
            }
            .validate()
            .is_ok());
        }
        assert!(Request {
            target: "quickAdjust".into(),
            function: "showBrightnessLevel".into(),
            args: vec!["x".into()]
        }
        .validate()
        .is_err());
    }
    #[test]
    fn desktop_contract_for_patched_hypr_scripts() {
        let req = |f: &str, a: &[&str]| Request {
            target: "desktop".into(),
            function: f.into(),
            args: a.iter().map(|s| s.to_string()).collect(),
        };
        for v in ["v1", "v2", "v3", "bt"] {
            assert!(req("setVariant", &[v]).validate().is_ok(), "{v}");
        }
        assert!(req("setVariant", &["daily"]).validate().is_err());
        assert!(req("setVariant", &[]).validate().is_err());
        assert!(req("deskflowReflow", &[]).validate().is_ok());
        assert!(req("deskflowReflow", &["x"]).validate().is_err());
    }
    #[tokio::test(flavor = "current_thread")]
    async fn roundtrip_lock_and_cleanup() {
        let path = path("roundtrip");
        let server = IpcServer::bind(path.clone()).unwrap();
        assert!(InstanceLock::acquire(&path).is_err());
        let (stop, signal) = watch::channel(false);
        let task = tokio::spawn(async move {
            server
                .run(
                    Arc::new(|r| Box::pin(async move { Response::success(r.function) })),
                    signal,
                )
                .await
                .unwrap();
        });
        let response = call(
            &path,
            &Request {
                target: "calendar".into(),
                function: "toggle".into(),
                args: vec![],
            },
        )
        .await
        .unwrap();
        assert!(response.ok);
        assert_eq!(response.result, "toggle");
        stop.send_replace(true);
        task.await.unwrap();
        assert!(!path.exists());
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn refuses_regular_socket_path() {
        let path = path("regular");
        let lock = InstanceLock::acquire(&path).unwrap();
        drop(lock);
        std::fs::write(&path, "keep").unwrap();
        assert!(IpcServer::bind(path.clone()).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "keep");
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
