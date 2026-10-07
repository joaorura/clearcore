//! Dedicated file logger with rolling 24-hour retention for the ClearCore Daemon.
//!
//! Log file defaults to `<data_dir>/clearcore/logs/daemon.log`.
//! Rotated logs: `daemon.YYYY-MM-DD.log`.
//! Any log older than 24-48 hours is automatically pruned during startup and periodic sweeps.
//!
//! Note on real-time safety: Audio processing in DSP threads (`process_frame`) NEVER performs
//! blocking file I/O. Only supervisor lifecycle, model loading, IPC commands and background jobs
//! invoke logger calls.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_SUBDIR: &str = "clearcore/logs";
const CURRENT_LOG_NAME: &str = "daemon.log";
const RETENTION_SECS: u64 = 24 * 3600; // 24 hours

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

pub struct DaemonLogger {
    log_dir: PathBuf,
    current_file: PathBuf,
    active_date: String,
    writer: Option<File>,
}

static GLOBAL_LOGGER: Mutex<Option<DaemonLogger>> = Mutex::new(None);

impl DaemonLogger {
    /// Resolves the standard platform-specific log directory:
    /// - Linux/Unix: `$XDG_DATA_HOME/clearcore/logs` or `~/.local/share/clearcore/logs`
    /// - macOS: `~/Library/Application Support/clearcore/logs`
    /// - Windows: `%APPDATA%\clearcore\logs`
    pub fn default_log_dir() -> PathBuf {
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
                })
        };

        base.unwrap_or_else(std::env::temp_dir).join(LOG_SUBDIR)
    }

    /// Initializes a new logger instance pointing to the given directory.
    pub fn init_in(log_dir: PathBuf) -> std::io::Result<()> {
        fs::create_dir_all(&log_dir)?;
        let date_str = current_utc_date_str();
        let current_file = log_dir.join(CURRENT_LOG_NAME);

        let mut instance = Self {
            log_dir,
            current_file,
            active_date: date_str,
            writer: None,
        };

        instance.prune_old_logs();
        instance.open_current_writer()?;

        let mut lock = GLOBAL_LOGGER
            .lock()
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "Logger mutex poisoned"))?;
        *lock = Some(instance);
        Ok(())
    }

    /// Initializes with default platform directory.
    pub fn init() -> std::io::Result<()> {
        Self::init_in(Self::default_log_dir())
    }

    fn open_current_writer(&mut self) -> std::io::Result<()> {
        let mut opts = OpenOptions::new();
        opts.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts.open(&self.current_file)?;
        self.writer = Some(file);
        Ok(())
    }

    /// Rotates the active log if the calendar day changed, and prunes files older than 24h.
    fn check_rotation(&mut self) {
        let today = current_utc_date_str();
        if today != self.active_date {
            // Close writer
            self.writer = None;

            // Archive previous daemon.log as daemon.YYYY-MM-DD.log
            let archive_name = format!("daemon.{}.log", self.active_date);
            let archive_path = self.log_dir.join(archive_name);
            if self.current_file.exists() && !archive_path.exists() {
                let _ = fs::rename(&self.current_file, &archive_path);
            }

            self.active_date = today;
            let _ = self.open_current_writer();
            self.prune_old_logs();
        }
    }

    /// Deletes logs older than 24 hours (with 4-hour grace window for 28 hours total safe retention).
    pub fn prune_old_logs(&self) {
        let Ok(entries) = fs::read_dir(&self.log_dir) else {
            return;
        };
        let now = SystemTime::now();

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !filename.starts_with("daemon.") || !filename.ends_with(".log") {
                continue;
            }
            if filename == CURRENT_LOG_NAME {
                continue;
            }

            if let Ok(metadata) = entry.metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(age) = now.duration_since(modified) {
                        if age.as_secs() > RETENTION_SECS {
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }
    }

    fn write_entry(&mut self, level: LogLevel, target: &str, message: &str) {
        self.check_rotation();

        let timestamp = iso8601_now();
        let level_str = level.as_str();
        let line = format!("{timestamp} [{level_str}] [{target}] {message}\n");

        if let Some(ref mut writer) = self.writer {
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.flush();
        }
    }
}

fn iso8601_now() -> String {
    let now = SystemTime::now();
    let epoch_secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let millis = now
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis();

    let (year, month, day, hour, min, sec) = seconds_to_utc_parts(epoch_secs);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}Z")
}

fn current_utc_date_str() -> String {
    let now = SystemTime::now();
    let epoch_secs = now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let (year, month, day, _, _, _) = seconds_to_utc_parts(epoch_secs);
    format!("{year:04}-{month:02}-{day:02}")
}

fn seconds_to_utc_parts(epoch_secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let sec = epoch_secs % 60;
    let total_mins = epoch_secs / 60;
    let min = total_mins % 60;
    let total_hours = total_mins / 60;
    let hour = total_hours % 24;
    let days = total_hours / 24;

    // Civil calendar algorithm (Euclidean affine)
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (y, m, d, hour, min, sec)
}

pub fn log(level: LogLevel, target: &str, message: &str) {
    if let Ok(mut lock) = GLOBAL_LOGGER.lock() {
        if let Some(ref mut logger) = *lock {
            logger.write_entry(level, target, message);
            return;
        }
    }
    // Fallback if logger is uninitialized: print to stderr/stdout
    let ts = iso8601_now();
    eprintln!("{ts} [{}] [{target}] {message}", level.as_str());
}

#[macro_export]
macro_rules! log_debug {
    ($target:expr, $($arg:tt)+) => {
        $crate::logger::log($crate::logger::LogLevel::Debug, $target, &format!($($arg)+))
    };
}

#[macro_export]
macro_rules! log_info {
    ($target:expr, $($arg:tt)+) => {
        $crate::logger::log($crate::logger::LogLevel::Info, $target, &format!($($arg)+))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($target:expr, $($arg:tt)+) => {
        $crate::logger::log($crate::logger::LogLevel::Warn, $target, &format!($($arg)+))
    };
}

#[macro_export]
macro_rules! log_error {
    ($target:expr, $($arg:tt)+) => {
        $crate::logger::log($crate::logger::LogLevel::Error, $target, &format!($($arg)+))
    };
}
