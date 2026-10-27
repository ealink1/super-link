//! Shared in-process SSH + SFTP fixture for integration tests.
//!
//! Each integration test compiles this module separately, so allow the items
//! that only one of them uses.
#![allow(dead_code)]
//!
//! The host key below is a throwaway fixture generated for these tests; it only
//! protects a loopback listener that the test itself starts and stops.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use russh::keys::{HashAlg, PrivateKey};
use russh::server::{self, Auth, Msg, Server as _, Session};
use russh::{Channel, ChannelId, Pty};
use russh_sftp::protocol::{
    Attrs, Data, File, FileAttributes, Handle, Name, OpenFlags, Status, StatusCode,
};
use russh_sftp::server::Handler as SftpHandlerTrait;
use superlink_domain::ShellHost;

pub const HOST_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACAmpRrzqA1+WJztXgOJEHOW/wz8NExGujvNp6kRV62uXgAAAJjKmuH5yprh
+QAAAAtzc2gtZWQyNTUxOQAAACAmpRrzqA1+WJztXgOJEHOW/wz8NExGujvNp6kRV62uXg
AAAEDBf68nBZAitWXPsKyJXRCJcmshc/IgJVa+JMtyUOogGyalGvOoDX5YnO1eA4kQc5b/
DPw0TEa6O82nqRFXra5eAAAADnN1cGVybGluay10ZXN0AQIDBAUGBw==
-----END OPENSSH PRIVATE KEY-----
";

pub const METRICS: &str = "Linux
__LOAD__
0.1 0.2 0.3 1/10 42
__MEM__
MemTotal: 1000 kB
MemAvailable: 400 kB
__UPTIME__
123.0 50.0
__DISK__
Filesystem Blocks Used Available Capacity Mounted
/dev/root 100 20 80 20% /
";

#[derive(Default)]
pub struct Shared {
    pub resizes: Vec<(u32, u32)>,
    pub channels: usize,
    pub terminals: Vec<String>,
    pub reject_pty: bool,
}

#[derive(Default)]
pub struct Options {
    pub reject_pty: bool,
    pub sftp: bool,
}

struct FixtureHandler {
    shared: Arc<Mutex<Shared>>,
    password: String,
    root: Option<PathBuf>,
    channels: HashMap<ChannelId, Channel<Msg>>,
    sftp_channels: HashSet<ChannelId>,
}

impl server::Server for FixtureHandler {
    type Handler = Self;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            password: self.password.clone(),
            root: self.root.clone(),
            channels: HashMap::new(),
            sftp_channels: HashSet::new(),
        }
    }
}

impl server::Handler for FixtureHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, _user: &str, password: &str) -> Result<Auth, Self::Error> {
        if password == self.password {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: server::ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shared.lock().unwrap().channels += 1;
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        term: &str,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let reject = {
            let mut shared = self.shared.lock().unwrap();
            shared.terminals.push(term.to_owned());
            if !shared.reject_pty {
                shared.resizes.push((col_width, row_height));
            }
            shared.reject_pty
        };
        if reject {
            let _ = session.channel_failure(channel);
        } else {
            let _ = session.channel_success(channel);
        }
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let _ = session.channel_success(channel);
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _channel: ChannelId,
        col_width: u32,
        row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.shared
            .lock()
            .unwrap()
            .resizes
            .push((col_width, row_height));
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        // SFTP data belongs to the subsystem reader, not to the echo terminal.
        if !self.sftp_channels.contains(&channel) {
            session.data(channel, data.to_vec())?;
        }
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let _ = session.channel_success(channel);
        let command = String::from_utf8_lossy(data);
        let output: Vec<u8> = if command.contains("uname") {
            METRICS.as_bytes().to_vec()
        } else if command.contains("find ") {
            b"10\t/tmp/big\0".to_vec()
        } else {
            Vec::new()
        };
        session.data(channel, output)?;
        let _ = session.exit_status_request(channel, 0);
        let _ = session.eof(channel);
        let _ = session.close(channel);
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Some(channel) = self.channels.remove(&channel) else {
            let _ = session.channel_failure(channel);
            return Ok(());
        };
        let Some(root) = self.root.clone() else {
            let _ = session.channel_failure(channel.id());
            return Ok(());
        };
        if name != "sftp" {
            let _ = session.channel_failure(channel.id());
            return Ok(());
        }
        session.channel_success(channel.id())?;
        self.sftp_channels.insert(channel.id());
        russh_sftp::server::run(channel.into_stream(), SftpFixture::new(root)).await;
        Ok(())
    }
}

struct SftpFixture {
    root: PathBuf,
    files: HashMap<String, std::fs::File>,
    directories: HashMap<String, (Vec<File>, bool)>,
    next_handle: u64,
}

impl SftpFixture {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: HashMap::new(),
            directories: HashMap::new(),
            next_handle: 0,
        }
    }

    fn handle(&mut self) -> String {
        self.next_handle += 1;
        format!("h{}", self.next_handle)
    }

    fn local(&self, path: &str) -> Result<PathBuf, StatusCode> {
        let trimmed = path.trim_start_matches('/');
        if trimmed.split('/').any(|part| part == "..") {
            return Err(StatusCode::PermissionDenied);
        }
        Ok(if trimmed.is_empty() {
            self.root.clone()
        } else {
            self.root.join(trimmed)
        })
    }

    fn entry(path: &Path, name: &str) -> Result<File, StatusCode> {
        let metadata = path.symlink_metadata().map_err(io_status)?;
        Ok(File::new(name, attributes(&metadata)))
    }
}

impl SftpHandlerTrait for SftpFixture {
    type Error = StatusCode;

    fn unimplemented(&self) -> Self::Error {
        StatusCode::OpUnsupported
    }

    async fn open(
        &mut self,
        id: u32,
        filename: String,
        flags: OpenFlags,
        _attrs: FileAttributes,
    ) -> Result<Handle, StatusCode> {
        let path = self.local(&filename)?;
        let mut options = std::fs::OpenOptions::new();
        if flags.contains(OpenFlags::READ) {
            options.read(true);
        }
        if flags.contains(OpenFlags::WRITE) {
            options.write(true);
        }
        if flags.contains(OpenFlags::CREATE) {
            if flags.contains(OpenFlags::EXCLUDE) {
                options.create_new(true);
            } else {
                options.create(true);
            }
        }
        if flags.contains(OpenFlags::TRUNCATE) {
            options.truncate(true);
        }
        let file = options.open(path).map_err(io_status)?;
        let handle = self.handle();
        self.files.insert(handle.clone(), file);
        Ok(Handle { id, handle })
    }

    async fn close(&mut self, id: u32, handle: String) -> Result<Status, StatusCode> {
        self.files.remove(&handle);
        Ok(ok(id))
    }

    async fn read(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        len: u32,
    ) -> Result<Data, StatusCode> {
        let file = self.files.get_mut(&handle).ok_or(StatusCode::Failure)?;
        file.seek(SeekFrom::Start(offset)).map_err(io_status)?;
        let mut buffer = vec![0u8; usize::min(len as usize, 64 << 10)];
        let read = file.read(&mut buffer).map_err(io_status)?;
        if read == 0 {
            return Err(StatusCode::Eof);
        }
        buffer.truncate(read);
        Ok(Data { id, data: buffer })
    }

    async fn write(
        &mut self,
        id: u32,
        handle: String,
        offset: u64,
        data: Vec<u8>,
    ) -> Result<Status, StatusCode> {
        let file = self.files.get_mut(&handle).ok_or(StatusCode::Failure)?;
        file.seek(SeekFrom::Start(offset)).map_err(io_status)?;
        file.write_all(&data).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn lstat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let path = self.local(&path)?;
        let metadata = path.symlink_metadata().map_err(io_status)?;
        Ok(Attrs {
            id,
            attrs: attributes(&metadata),
        })
    }

    async fn stat(&mut self, id: u32, path: String) -> Result<Attrs, StatusCode> {
        let path = self.local(&path)?;
        let metadata = std::fs::metadata(path).map_err(io_status)?;
        Ok(Attrs {
            id,
            attrs: attributes(&metadata),
        })
    }

    async fn fstat(&mut self, id: u32, handle: String) -> Result<Attrs, StatusCode> {
        let file = self.files.get(&handle).ok_or(StatusCode::Failure)?;
        let metadata = file.metadata().map_err(io_status)?;
        Ok(Attrs {
            id,
            attrs: attributes(&metadata),
        })
    }

    async fn setstat(
        &mut self,
        id: u32,
        path: String,
        attrs: FileAttributes,
    ) -> Result<Status, StatusCode> {
        let path = self.local(&path)?;
        if let Some(mode) = attrs.permissions {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o777))
                .map_err(io_status)?;
        }
        Ok(ok(id))
    }

    async fn opendir(&mut self, id: u32, path: String) -> Result<Handle, StatusCode> {
        let path = self.local(&path)?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path).map_err(io_status)? {
            let entry = entry.map_err(io_status)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            entries.push(SftpFixture::entry(&entry.path(), &name)?);
        }
        let handle = self.handle();
        self.directories.insert(handle.clone(), (entries, false));
        Ok(Handle { id, handle })
    }

    async fn readdir(&mut self, id: u32, handle: String) -> Result<Name, StatusCode> {
        let Some((files, done)) = self.directories.get_mut(&handle) else {
            return Err(StatusCode::Failure);
        };
        if *done {
            return Err(StatusCode::Eof);
        }
        *done = true;
        Ok(Name {
            id,
            files: std::mem::take(files),
        })
    }

    async fn rename(
        &mut self,
        id: u32,
        oldpath: String,
        newpath: String,
    ) -> Result<Status, StatusCode> {
        let source = self.local(&oldpath)?;
        let destination = self.local(&newpath)?;
        if destination.symlink_metadata().is_ok() {
            return Err(StatusCode::Failure);
        }
        std::fs::rename(source, destination).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn remove(&mut self, id: u32, filename: String) -> Result<Status, StatusCode> {
        let path = self.local(&filename)?;
        std::fs::remove_file(path).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn rmdir(&mut self, id: u32, path: String) -> Result<Status, StatusCode> {
        let path = self.local(&path)?;
        std::fs::remove_dir(path).map_err(io_status)?;
        Ok(ok(id))
    }

    async fn realpath(&mut self, id: u32, path: String) -> Result<Name, StatusCode> {
        let path = self.local(&path)?;
        let absolute = std::fs::canonicalize(path).map_err(io_status)?;
        Ok(Name {
            id,
            files: vec![File::dummy(absolute.to_string_lossy().into_owned())],
        })
    }
}

fn ok(id: u32) -> Status {
    Status {
        id,
        status_code: StatusCode::Ok,
        error_message: "Ok".to_owned(),
        language_tag: "en-US".to_owned(),
    }
}

fn io_status(error: std::io::Error) -> StatusCode {
    match error.kind() {
        std::io::ErrorKind::NotFound => StatusCode::NoSuchFile,
        std::io::ErrorKind::PermissionDenied => StatusCode::PermissionDenied,
        std::io::ErrorKind::AlreadyExists => StatusCode::Failure,
        _ => StatusCode::Failure,
    }
}

fn attributes(metadata: &std::fs::Metadata) -> FileAttributes {
    use std::os::unix::fs::MetadataExt;
    let mut attrs = FileAttributes::empty();
    attrs.size = Some(metadata.len());
    attrs.permissions = Some(metadata.mode());
    attrs.mtime = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_secs() as u32);
    attrs
}

pub struct FixtureServer {
    address: SocketAddr,
    pub shared: Arc<Mutex<Shared>>,
    root: PathBuf,
    shutdown: server::RunningServerHandle,
    runtime: Option<tokio::runtime::Runtime>,
}

impl FixtureServer {
    pub fn start() -> Self {
        Self::start_with(Options::default())
    }

    pub fn start_with_sftp() -> Self {
        Self::start_with(Options {
            sftp: true,
            ..Options::default()
        })
    }

    pub fn start_with_rejecting_pty() -> Self {
        Self::start_with(Options {
            reject_pty: true,
            ..Options::default()
        })
    }

    pub fn start_with(options: Options) -> Self {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("fixture runtime");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        listener.set_nonblocking(true).expect("nonblocking fixture");
        let address = listener.local_addr().expect("fixture address");
        let shared = Arc::new(Mutex::new(Shared {
            reject_pty: options.reject_pty,
            ..Shared::default()
        }));
        let root = temporary_root();
        let root_for_server = if options.sftp {
            Some(root.clone())
        } else {
            None
        };
        let task_shared = Arc::clone(&shared);
        let (send_handle, receive_handle) = std::sync::mpsc::channel();
        runtime.spawn(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("tokio listener");
            let host_key = PrivateKey::from_openssh(HOST_KEY).expect("fixture host key");
            let config = Arc::new(server::Config {
                keys: vec![host_key],
                inactivity_timeout: Some(Duration::from_secs(60)),
                auth_rejection_time: Duration::from_millis(10),
                auth_rejection_time_initial: Some(Duration::from_millis(0)),
                ..server::Config::default()
            });
            let mut fixture = FixtureHandler {
                shared: task_shared,
                password: "fixture".to_owned(),
                root: root_for_server,
                channels: HashMap::new(),
                sftp_channels: HashSet::new(),
            };
            let running = fixture.run_on_socket(config, &listener);
            send_handle.send(running.handle()).expect("send handle");
            let _ = running.await;
        });
        let shutdown = receive_handle
            .recv_timeout(Duration::from_secs(5))
            .expect("fixture server started");
        Self {
            address,
            shared,
            root,
            shutdown,
            runtime: Some(runtime),
        }
    }

    pub fn host(&self) -> ShellHost {
        ShellHost {
            name: "fixture".to_owned(),
            host: "127.0.0.1".to_owned(),
            user: "tester".to_owned(),
            port: self.address.port(),
            password: "fixture".to_owned(),
            ..ShellHost::default()
        }
    }

    pub fn pin() -> String {
        PrivateKey::from_openssh(HOST_KEY)
            .expect("fixture host key")
            .public_key()
            .fingerprint(HashAlg::Sha256)
            .to_string()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn wait_for_resize(&self, expected: (u32, u32)) {
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if self.shared.lock().unwrap().resizes.contains(&expected) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "resize {expected:?} was never reported"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.shutdown.shutdown("test finished".to_owned());
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn temporary_root() -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    // Tests run in parallel inside one process: a clock-based name alone can
    // collide, and then one fixture would delete the root of another.
    let root = std::env::temp_dir().join(format!(
        "superlink-fixture-{}-{unique}-{sequence}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("fixture root");
    root
}
