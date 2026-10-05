#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub const SYSTEMD_SERVICE_NAME: &str = "realtime-noise.service";
pub const LAUNCHD_PLIST_NAME: &str = "com.clearcore.realtime-noise.plist";
pub const WINDOWS_TASK_NAME: &str = "RealtimeNoiseService";

/// Generates a systemd user unit definition for Linux.
#[must_use]
pub fn generate_systemd_unit(bin_path: &Path) -> String {
    format!(
        r"[Unit]
Description=Realtime Noise Suppression User Service
After=sound.target pipewire.service pulseaudio.service
PartOf=graphical-session.target

[Service]
Type=simple
ExecStart={} --run
Restart=on-failure
RestartSec=2s
LimitRTPRIO=95
LimitMEMLOCK=infinity
LimitCORE=0

[Install]
WantedBy=default.target
",
        bin_path.display()
    )
}

/// Generates a `LaunchAgent` plist definition for macOS.
#[must_use]
pub fn generate_launchd_plist(bin_path: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.clearcore.realtime-noise</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--run</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>ProcessType</key>
    <string>Interactive</string>
</dict>
</plist>
"#,
        bin_path.display()
    )
}

/// Generates the Windows scheduled task registration command.
#[must_use]
pub fn generate_windows_task_cmd(bin_path: &Path) -> String {
    format!(
        r#"schtasks.exe /create /tn "{WINDOWS_TASK_NAME}" /tr "\"{}\" --run" /sc onlogon /rl limited /f"#,
        bin_path.display()
    )
}

/// Installs the per-user service on the current platform.
pub fn install_user_service() -> io::Result<()> {
    let current_exe = std::env::current_exe()?;

    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let unit_dir = PathBuf::from(home).join(".config/systemd/user");
            fs::create_dir_all(&unit_dir)?;
            let unit_file = unit_dir.join(SYSTEMD_SERVICE_NAME);
            let content = generate_systemd_unit(&current_exe);
            fs::write(&unit_file, content)?;
            println!("Installed systemd user service to {}", unit_file.display());
            println!(
                "To activate now, run: systemctl --user daemon-reload && systemctl --user enable --now {SYSTEMD_SERVICE_NAME}"
            );
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "HOME environment variable not set",
            ));
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let agent_dir = PathBuf::from(home).join("Library/LaunchAgents");
            fs::create_dir_all(&agent_dir)?;
            let plist_file = agent_dir.join(LAUNCHD_PLIST_NAME);
            let content = generate_launchd_plist(&current_exe);
            fs::write(&plist_file, content)?;
            println!("Installed LaunchAgent to {}", plist_file.display());
            println!(
                "To activate now, run: launchctl load {}",
                plist_file.display()
            );
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "HOME environment variable not set",
            ));
        }
    }

    #[cfg(target_os = "windows")]
    {
        println!("Registering Windows scheduled task on logon: {WINDOWS_TASK_NAME}");
        let cmd = generate_windows_task_cmd(&current_exe);
        println!("Run command in elevated or user PowerShell: {cmd}");
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        println!(
            "Unsupported platform for automated service installation. Current binary: {}",
            current_exe.display()
        );
    }

    Ok(())
}

/// Uninstalls the per-user service on the current platform.
pub fn uninstall_user_service() -> io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let unit_file = PathBuf::from(home)
                .join(".config/systemd/user")
                .join(SYSTEMD_SERVICE_NAME);
            if unit_file.exists() {
                fs::remove_file(&unit_file)?;
                println!("Removed systemd user service from {}", unit_file.display());
                println!("To complete uninstallation, run: systemctl --user daemon-reload");
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let plist_file = PathBuf::from(home)
                .join("Library/LaunchAgents")
                .join(LAUNCHD_PLIST_NAME);
            if plist_file.exists() {
                fs::remove_file(&plist_file)?;
                println!("Removed LaunchAgent from {}", plist_file.display());
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        println!(
            "To unregister Windows scheduled task, run: schtasks.exe /delete /tn \"{WINDOWS_TASK_NAME}\" /f"
        );
    }

    Ok(())
}
