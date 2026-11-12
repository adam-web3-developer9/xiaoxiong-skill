use crate::*;
use interprocess::local_socket::{
    prelude::*, GenericFilePath, ListenerNonblockingMode, ListenerOptions, Stream,
};
use rand::RngCore;
use std::{
    fs,
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};
#[cfg(not(windows))]
use std::io::Read;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
    pub protocol: u8,
    pub endpoint: String,
    pub token: String,
    pub data_root: PathBuf,
    pub pid: u32,
}
impl std::fmt::Debug for Descriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Descriptor")
            .field("protocol", &self.protocol)
            .field("endpoint", &self.endpoint)
            .field("token", &"[redacted]")
            .field("data_root", &self.data_root)
            .field("pid", &self.pid)
            .finish()
    }
}
pub struct Options {
    pub data_root: PathBuf,
    pub temporary_directory: PathBuf,
}
pub struct Server {
    descriptor: Descriptor,
    stopping: Arc<AtomicBool>,
    worker: Option<JoinHandle<io::Result<()>>>,
}
impl Server {
    /// The supplied runtime executes the existing application's async methods.
    /// Closing is blocking: invoke it off the application's reactor thread.
    pub fn start(
        options: Options,
        host: Arc<dyn Host>,
        runtime: tokio::runtime::Handle,
    ) -> io::Result<Self> {
        Self::start_with_admission(options, host, runtime, Arc::new(AtomicBool::new(false)))
    }
    /// Share admission with the application lifecycle so stopping during
    /// listener construction also stops the unpublished listener.
    pub fn start_with_admission(
        options: Options,
        host: Arc<dyn Host>,
        runtime: tokio::runtime::Handle,
        stopping: Arc<AtomicBool>,
    ) -> io::Result<Self> {
        let root = resolve_path(&options.data_root)?;
        let mut random = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut random);
        let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let mut socket_random = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut socket_random);
        let socket_id: String = socket_random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let instance = uuid::Uuid::new_v4();
        let endpoint = if cfg!(windows) {
            format!(r"\\.\pipe\aistudio-agent-{instance}")
        } else {
            options
                .temporary_directory
                .join(format!("ai-{socket_id}.sock"))
                .to_string_lossy()
                .into_owned()
        };
        let mut directory = fs::DirBuilder::new();
        directory.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory.create(&root)?;
        let options = ListenerOptions::new()
            .name(endpoint.as_str().to_fs_name::<GenericFilePath>()?)
            .nonblocking(ListenerNonblockingMode::Both);
        #[cfg(windows)]
        let options = {
            use interprocess::os::windows::{
                local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
            };
            let sddl =
                widestring::U16CString::from_str("D:P(A;;GA;;;OW)").map_err(io::Error::other)?;
            options.security_descriptor(SecurityDescriptor::deserialize(&sddl)?)
        };
        let listener = options.create_sync()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
        }
        let descriptor = Descriptor {
            protocol: 1,
            endpoint,
            token,
            data_root: root.clone(),
            pid: std::process::id(),
        };
        let temporary = root.join(format!("agent-data-connection.json.{instance}.tmp"));
        let descriptor_path = root.join("agent-data-connection.json");
        let install = (|| -> io::Result<()> {
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut output = options.open(&temporary)?;
            output.write_all(&serde_json::to_vec(&descriptor)?)?;
            output.sync_all()?;
            drop(output);
            fs::rename(&temporary, &descriptor_path)
        })();
        if let Err(error) = install {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        let stop = stopping.clone();
        let owned = descriptor.clone();
        let worker = match thread::Builder::new()
            .name("aistudio-agent-accept".into())
            .spawn(move || {
                let mut clients: Vec<JoinHandle<()>> = vec![];
                let mut failed = None;
                while !stop.load(Ordering::Acquire) {
                    match listener.accept() {
                        Ok(stream) => {
                            if stop.load(Ordering::Acquire) {
                                break;
                            }
                            let host = host.clone();
                            let runtime = runtime.clone();
                            let stop = stop.clone();
                            let owned = owned.clone();
                            match thread::Builder::new()
                                .name("aistudio-agent-client".into())
                                .spawn(move || {
                                    let session = Session::new(owned.data_root, owned.token, host);
                                    // Transport failure never cancels an accepted invoke.
                                    let _ = serve(stream, session, &stop, &runtime);
                                }) {
                                Ok(client) => clients.push(client),
                                Err(error) => {
                                    failed = Some(error);
                                    break;
                                }
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(10))
                        }
                        Err(error) => {
                            failed = Some(error);
                            break;
                        }
                    }
                    let mut index = 0;
                    while index < clients.len() {
                        if clients[index].is_finished() {
                            let _ = clients.swap_remove(index).join();
                        } else {
                            index += 1;
                        }
                    }
                }
                // Stop idle readers, then wait for the currently accepted method on
                // every connection before letting the application close its stores.
                stop.store(true, Ordering::Release);
                for client in clients {
                    if client.join().is_err() && failed.is_none() {
                        failed = Some(io::Error::other("Agent client worker failed"));
                    }
                }
                drop(listener);
                match failed {
                    Some(error) => Err(error),
                    None => Ok(()),
                }
            }) {
            Ok(worker) => worker,
            Err(error) => {
                let _ = remove_descriptor(&descriptor);
                return Err(error);
            }
        };
        Ok(Self {
            descriptor,
            stopping,
            worker: Some(worker),
        })
    }
    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }
    /// Stop accepting new methods immediately; close() subsequently waits for
    /// the already accepted invocation without cancelling its native future.
    pub fn stop_admission(&self) {
        self.stopping.store(true, Ordering::Release);
    }
    pub fn close(&mut self) -> io::Result<()> {
        self.stop_admission();
        let drained = self
            .worker
            .take()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| io::Error::other("Agent accept worker failed"))
                    .and_then(|result| result)
            })
            .transpose();
        let cleaned = remove_descriptor(&self.descriptor);
        drained?;
        cleaned
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
fn remove_descriptor(owned: &Descriptor) -> io::Result<()> {
    let path = owned.data_root.join("agent-data-connection.json");
    let current: Value = match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => value,
            Err(_) => return Ok(()),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    if current["token"] == owned.token && current["endpoint"] == owned.endpoint {
        fs::remove_file(path)?;
    }
    Ok(())
}
fn write_reply(stream: &mut Stream, bytes: &[u8], stop: &AtomicBool) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        match stream.write(&bytes[offset..]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(size) => offset += size,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock && !stop.load(Ordering::Acquire) =>
            {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
#[cfg(windows)]
fn read_request(stream: &mut Stream, buffer: &mut [u8]) -> io::Result<usize> {
    use std::{ffi::c_void, os::windows::io::{AsHandle, AsRawHandle}};
    #[link(name = "kernel32")]
    extern "system" {
        fn ReadFile(handle: *mut c_void, buffer: *mut c_void, length: u32, read: *mut u32, overlapped: *mut c_void) -> i32;
    }
    let Stream::NamedPipe(pipe) = stream;
    let mut read = 0u32;
    // PIPE_NOWAIT returns ERROR_NO_DATA for an open but idle pipe. The
    // interprocess Read implementation maps that error to EOF on Windows.
    let success = unsafe { ReadFile(pipe.as_handle().as_raw_handle(), buffer.as_mut_ptr().cast(), buffer.len() as u32, &mut read, std::ptr::null_mut()) };
    if success != 0 { return Ok(read as usize); }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        Some(232) => Err(io::ErrorKind::WouldBlock.into()), // ERROR_NO_DATA
        Some(109 | 233) => Ok(0), // ERROR_BROKEN_PIPE / ERROR_PIPE_NOT_CONNECTED
        _ => Err(error),
    }
}
#[cfg(not(windows))]
fn read_request(stream: &mut Stream, buffer: &mut [u8]) -> io::Result<usize> {
    stream.read(buffer)
}
fn serve(
    mut stream: Stream,
    mut session: Session,
    stop: &AtomicBool,
    runtime: &tokio::runtime::Handle,
) -> io::Result<()> {
    stream.set_nonblocking(true)?;
    let mut pending = vec![];
    let mut chunk = [0u8; 8192];
    while !stop.load(Ordering::Acquire) {
        match read_request(&mut stream, &mut chunk) {
            Ok(0) => break,
            Ok(size) => {
                pending
                    .try_reserve(size)
                    .map_err(|error| io::Error::new(io::ErrorKind::OutOfMemory, error))?;
                pending.extend_from_slice(&chunk[..size]);
                while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                    if stop.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    // The method owns its completion from this point, even if
                    // close or peer disconnect happens while it is in flight.
                    let response = runtime.block_on(session.process_line(&pending[..newline]));
                    pending.drain(..=newline);
                    let mut bytes = serde_json::to_vec(&response)?;
                    bytes.push(b'\n');
                    write_reply(&mut stream, &bytes, stop)?;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(10))
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
