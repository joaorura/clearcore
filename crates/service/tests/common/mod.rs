#![allow(dead_code)]

use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse};
use realtime_noise_service::ServiceDaemon;
use serde_json::json;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// Diretório temporário removido ao sair do escopo (sem crates de terceiros).
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "clearcore-service-test-{label}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Envia um comando ao daemon por uma sessão em memória e devolve a resposta.
pub fn send(daemon: &mut ServiceDaemon, command: IpcCommand) -> IpcResponse {
    let request = IpcRequest::new(command, json!({}));
    let mut reader =
        Cursor::new(format!("{}\n", request.to_json().expect("serialize")).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(&mut reader, &mut writer)
        .expect("serve client");
    IpcResponse::from_json(String::from_utf8(writer.into_inner()).expect("utf8").trim())
        .expect("parse response")
}
