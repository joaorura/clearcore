#![forbid(unsafe_code)]

pub mod protocol;
pub mod server;

pub use protocol::{
    handle_request, DenoiseMode, IpcCommand, IpcErrorDetail, IpcRequest, IpcResponse, IpcStatus,
    PROTOCOL_VERSION,
};
pub use server::IpcServer;
