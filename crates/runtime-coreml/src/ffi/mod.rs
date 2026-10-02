#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(target_os = "macos"))]
pub mod stub;

#[cfg(target_os = "macos")]
pub use macos as platform;

#[cfg(not(target_os = "macos"))]
pub use stub as platform;
