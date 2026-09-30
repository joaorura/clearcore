#![forbid(unsafe_code)]

use std::io;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformServiceTarget {
    WindowsTask,
    MacosLaunchAgent,
    LinuxSystemdUser,
}

pub fn install_user_service() -> io::Result<()> {
    // Skeleton implementation
    Ok(())
}

pub fn uninstall_user_service() -> io::Result<()> {
    // Skeleton implementation
    Ok(())
}
