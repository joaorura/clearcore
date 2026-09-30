#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod protocol;
pub mod server;

pub use protocol::{
    handle_request, DenoiseMode, IpcCommand, IpcErrorDetail, IpcRequest, IpcResponse, IpcStatus,
    PROTOCOL_VERSION,
};
pub use server::{default_endpoint_path, IpcClient, IpcServer, DEFAULT_PIPE_NAME, DEFAULT_SOCKET_NAME};
