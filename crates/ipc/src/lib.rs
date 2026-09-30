#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod protocol;
pub mod server;

pub use protocol::{
    DenoiseMode, IpcCommand, IpcErrorDetail, IpcRequest, IpcResponse, IpcStatus, PROTOCOL_VERSION,
    handle_request,
};
pub use server::{
    DEFAULT_PIPE_NAME, DEFAULT_SOCKET_NAME, IpcClient, IpcServer, default_endpoint_path,
};
