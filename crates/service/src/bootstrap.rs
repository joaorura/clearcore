#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use crate::ServiceDaemon;
use realtime_noise_ipc::default_endpoint_path;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ServiceConfig {
    pub endpoint_path: PathBuf,
}

impl Default for ServiceConfig {
    fn default() -> Self {
        Self {
            endpoint_path: PathBuf::from(default_endpoint_path()),
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
        Self {
            config,
            daemon: ServiceDaemon::new(),
        }
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

        for stream_res in listener.incoming() {
            if self.daemon.is_shutdown() {
                break;
            }
            match stream_res {
                Ok(stream) => {
                    let reader = std::io::BufReader::new(stream.try_clone()?);
                    let writer = stream;
                    if let Err(e) = self.daemon.serve_client(reader, writer) {
                        eprintln!("Error handling client session: {e}");
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

        for stream_res in listener.incoming() {
            if self.daemon.is_shutdown() {
                break;
            }
            match stream_res {
                Ok(stream) => {
                    let reader = std::io::BufReader::new(stream.try_clone()?);
                    let writer = stream;
                    if let Err(e) = self.daemon.serve_client(reader, writer) {
                        eprintln!("Error handling client session: {e}");
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
