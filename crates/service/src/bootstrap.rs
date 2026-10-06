#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use crate::ServiceDaemon;
use realtime_noise_ipc::default_endpoint_path;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

/// Idle timeout for reads (and writes) on an accepted control connection.
///
/// The accept loop serves one client at a time, so a peer that connects and then sends nothing,
/// or sends a line without ever finishing it (slow-loris), would otherwise block every other
/// client. A read that waits longer than this ends that session with an I/O error; the daemon
/// keeps accepting. The timeout is per read call, so a peer trickling bytes is still bounded by
/// the request line cap (`MAX_REQUEST_LINE_BYTES`). Writes get the same bound, so a peer that
/// never reads its responses cannot stall the loop either.
pub const CLIENT_IO_TIMEOUT: Duration = Duration::from_secs(30);

/// Fixed log line for a failed session: the I/O error text is never logged.
const SESSION_ERROR_MESSAGE: &str = "Client session ended with an I/O error";

/// The two socket options [`apply_client_timeouts`] sets; implemented by the stream types the
/// daemon accepts (and by a recorder in the tests).
pub(crate) trait ClientTimeouts {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()>;
}

#[cfg(unix)]
impl ClientTimeouts for std::os::unix::net::UnixStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        Self::set_read_timeout(self, timeout)
    }
    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        Self::set_write_timeout(self, timeout)
    }
}

impl ClientTimeouts for std::net::TcpStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        Self::set_read_timeout(self, timeout)
    }
    fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
        Self::set_write_timeout(self, timeout)
    }
}

/// Bounds how long an accepted connection may sit idle (see [`CLIENT_IO_TIMEOUT`]). Called on
/// every accepted stream before it is cloned, so the reader and the writer share the options.
pub(crate) fn apply_client_timeouts<S: ClientTimeouts>(stream: &S) -> io::Result<()> {
    stream.set_read_timeout(Some(CLIENT_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(CLIENT_IO_TIMEOUT))
}

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub endpoint_path: PathBuf,
    pub settings_path: PathBuf,
    pub model_dir: Option<PathBuf>,
    pub repo_root: Option<PathBuf>,
    pub initial_backend: Option<String>,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            endpoint_path: PathBuf::from(default_endpoint_path()),
            settings_path: crate::settings::default_settings_path(),
            model_dir: None,
            repo_root: None,
            initial_backend: None,
        }
    }
}

pub struct ServiceBootstrap {
    config: ServiceConfig,
    daemon: ServiceDaemon,
}

impl Default for ServiceBootstrap {
    fn default() -> Self {
        Self::new(ServiceConfig::default())
    }
}

impl ServiceBootstrap {
    #[must_use]
    pub fn new(config: ServiceConfig) -> Self {
        let settings = crate::settings::Settings::load(&config.settings_path);
        let mut daemon = ServiceDaemon::with_settings(settings.clone(), Some(config.settings_path.clone()));
        if let Some(ref model_dir) = config.model_dir {
            daemon.set_model_dir(model_dir.clone());
        }
        if let Some(ref repo_root) = config.repo_root {
            daemon.set_repo_root(repo_root.clone());
        }
        if let Some(dir) = realtime_noise_model::ProfileStore::default_dir() {
            daemon.attach_profile_store(realtime_noise_model::ProfileStore::new(dir));
        }
        let backend_to_select = config
            .initial_backend
            .as_deref()
            .or(settings.backend.as_deref());
        if let Some(backend) = backend_to_select {
            let _ = daemon.select_backend(backend);
        }
        Self { config, daemon }
    }

    #[must_use]
    pub fn config(&self) -> &ServiceConfig {
        &self.config
    }

    pub fn daemon(&self) -> &ServiceDaemon {
        &self.daemon
    }

    pub fn daemon_mut(&mut self) -> &mut ServiceDaemon {
        &mut self.daemon
    }

    #[cfg(unix)]
    pub fn run(&mut self) -> io::Result<()> {
        let socket_path = &self.config.endpoint_path;
        if socket_path.exists() {
            let _ = std::fs::remove_file(socket_path);
        }
        if let Some(parent) = socket_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let listener = std::os::unix::net::UnixListener::bind(socket_path)?;
        println!(
            "Service listening on unix domain socket: {}",
            socket_path.display()
        );
        println!(
            "ClearCore daemon active backend: {} (accelerated: {})",
            self.daemon.supervisor().active_backend_name(),
            self.daemon.supervisor().is_hardware_accelerated()
        );

        for stream_res in listener.incoming() {
            if self.daemon.is_shutdown() {
                break;
            }
            match stream_res {
                Ok(stream) => {
                    if apply_client_timeouts(&stream).is_err() {
                        eprintln!("Refused a client connection: its timeouts could not be set");
                        continue;
                    }
                    let reader = std::io::BufReader::new(stream.try_clone()?);
                    let writer = stream;
                    if self.daemon.serve_client(reader, writer).is_err() {
                        eprintln!("{SESSION_ERROR_MESSAGE}");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to accept connection: {e}");
                }
            }
        }

        if socket_path.exists() {
            let _ = std::fs::remove_file(socket_path);
        }
        println!("Service stopped cleanly.");
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn run(&mut self) -> io::Result<()> {
        let bind_addr = "127.0.0.1:49215";
        let listener = std::net::TcpListener::bind(bind_addr)?;
        println!("Service listening on TCP localhost: {bind_addr}");
        println!(
            "ClearCore daemon active backend: {} (accelerated: {})",
            self.daemon.supervisor().active_backend_name(),
            self.daemon.supervisor().is_hardware_accelerated()
        );

        for stream_res in listener.incoming() {
            if self.daemon.is_shutdown() {
                break;
            }
            match stream_res {
                Ok(stream) => {
                    if apply_client_timeouts(&stream).is_err() {
                        eprintln!("Refused a client connection: its timeouts could not be set");
                        continue;
                    }
                    let reader = std::io::BufReader::new(stream.try_clone()?);
                    let writer = stream;
                    if self.daemon.serve_client(reader, writer).is_err() {
                        eprintln!("{SESSION_ERROR_MESSAGE}");
                    }
                }
                Err(e) => {
                    eprintln!("Failed to accept connection: {e}");
                }
            }
        }

        println!("Service stopped cleanly.");
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct Recorder {
        read: RefCell<Vec<Option<Duration>>>,
        write: RefCell<Vec<Option<Duration>>>,
    }

    impl ClientTimeouts for Recorder {
        fn set_read_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            self.read.borrow_mut().push(timeout);
            Ok(())
        }
        fn set_write_timeout(&self, timeout: Option<Duration>) -> io::Result<()> {
            self.write.borrow_mut().push(timeout);
            Ok(())
        }
    }

    #[test]
    fn accepted_streams_get_a_bounded_idle_timeout() {
        assert!(CLIENT_IO_TIMEOUT >= Duration::from_secs(5));
        assert!(CLIENT_IO_TIMEOUT <= Duration::from_secs(60));
        let recorder = Recorder::default();
        apply_client_timeouts(&recorder).unwrap();
        assert_eq!(*recorder.read.borrow(), vec![Some(CLIENT_IO_TIMEOUT)]);
        assert_eq!(*recorder.write.borrow(), vec![Some(CLIENT_IO_TIMEOUT)]);
    }

    #[test]
    fn a_failing_option_is_reported() {
        struct Failing;
        impl ClientTimeouts for Failing {
            fn set_read_timeout(&self, _: Option<Duration>) -> io::Result<()> {
                Err(io::Error::other("no"))
            }
            fn set_write_timeout(&self, _: Option<Duration>) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(apply_client_timeouts(&Failing).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn real_unix_stream_accepts_the_timeouts() {
        let (a, _b) = std::os::unix::net::UnixStream::pair().unwrap();
        apply_client_timeouts(&a).unwrap();
        assert_eq!(a.read_timeout().unwrap(), Some(CLIENT_IO_TIMEOUT));
        assert_eq!(a.write_timeout().unwrap(), Some(CLIENT_IO_TIMEOUT));
    }
}
