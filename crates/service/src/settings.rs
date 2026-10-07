#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

//! Configuração persistida do serviço (`settings.json`).
//!
//! Leitura tolerante (qualquer problema vira os padrões) e escrita atômica
//! (arquivo temporário + `rename`), com permissão 0600 em unix.

use crate::{log_error, log_info, log_warn};
use realtime_noise_ipc::StudioPreset;
pub use realtime_noise_supervisor::{convert_dsp_preset_to_ipc, convert_ipc_preset_to_dsp};
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use studio_dsp::Preset;

pub const SETTINGS_VERSION: u32 = 1;
pub const SETTINGS_FILE_NAME: &str = "settings.json";

const SETTINGS_DIR_NAME: &str = "clearcore";
const FALLBACK_FILE_NAME: &str = "clearcore-settings.json";
const TEMP_SUFFIX: &str = ".tmp";

/// Configuração persistida. Contém o preset de acabamento de estúdio e o backend selecionado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    pub version: u32,
    pub preset: Preset,
    pub backend: Option<String>,
    /// Suppression intensity 0–100 (maps to `post_filter_beta` 0.0–0.10).
    pub filter_intensity: u8,
    /// Whether voice isolation (biometric profile) is active.
    pub voice_isolation_enabled: bool,
    /// Voice auto-leveler intensity 0–100 (0 = bypass, 50 = balanced, 100 = firm).
    pub voice_leveler: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            preset: Preset::Off,
            backend: None,
            filter_intensity: 50,
            voice_isolation_enabled: true,
            voice_leveler: 0,
        }
    }
}

/// Forma em disco. Usa o tipo de fio `StudioPreset` como única fonte dos nomes.
#[derive(Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    #[serde(default)]
    preset: StudioPreset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    backend: Option<String>,
    #[serde(default = "default_filter_intensity")]
    filter_intensity: u8,
    #[serde(default = "default_voice_isolation")]
    voice_isolation_enabled: bool,
    #[serde(default = "default_voice_leveler")]
    voice_leveler: u8,
}

fn default_filter_intensity() -> u8 {
    50
}

fn default_voice_isolation() -> bool {
    true
}

fn default_voice_leveler() -> u8 {
    0
}

impl Settings {
    /// Interpreta o texto de um `settings.json`; qualquer problema vira os padrões.
    #[must_use]
    pub fn parse_or_default(text: &str) -> Self {
        match serde_json::from_str::<SettingsFile>(text) {
            Ok(file) if (1..=SETTINGS_VERSION).contains(&file.version) => Self {
                version: SETTINGS_VERSION,
                preset: convert_ipc_preset_to_dsp(file.preset),
                backend: file.backend,
                filter_intensity: file.filter_intensity.min(100),
                voice_isolation_enabled: file.voice_isolation_enabled,
                voice_leveler: file.voice_leveler.min(100),
            },
            _ => Self::default(),
        }
    }

    /// Lê o arquivo; ausente, ilegível ou inválido resulta nos padrões.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => {
                let parsed = Self::parse_or_default(&text);
                log_info!(
                    "SETTINGS",
                    "Loaded settings from {}: preset={:?}, backend={:?}",
                    path.display(),
                    parsed.preset,
                    parsed.backend
                );
                parsed
            }
            Err(err) => {
                if err.kind() != io::ErrorKind::NotFound {
                    log_warn!(
                        "SETTINGS",
                        "Could not read settings file {}: {err}; using defaults",
                        path.display()
                    );
                    eprintln!(
                        "Could not read settings file {}: {err}; using defaults",
                        path.display()
                    );
                } else {
                    log_info!(
                        "SETTINGS",
                        "Settings file not found at {}; using defaults",
                        path.display()
                    );
                }
                Self::default()
            }
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(&SettingsFile {
            version: SETTINGS_VERSION,
            preset: convert_dsp_preset_to_ipc(self.preset),
            backend: self.backend.clone(),
            filter_intensity: self.filter_intensity,
            voice_isolation_enabled: self.voice_isolation_enabled,
            voice_leveler: self.voice_leveler,
        })
    }

    /// Escrita atômica: temporário no mesmo diretório + `rename`.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        let json = self
            .to_json()
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;

        match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => fs::create_dir_all(parent)?,
            _ => {}
        }

        let temp_path = temp_path_for(path);
        let _ = fs::remove_file(&temp_path);
        let result = write_private_file(&temp_path, json.as_bytes())
            .and_then(|()| fs::rename(&temp_path, path));
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}

/// Persiste sem propagar erro: devolve `true` se gravou. `None` desliga a persistência.
#[must_use]
pub fn persist_settings(settings: Settings, path: Option<&Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    match settings.save(path) {
        Ok(()) => {
            log_info!(
                "SETTINGS",
                "Saved settings to {}: preset={:?}, backend={:?}",
                path.display(),
                settings.preset,
                settings.backend
            );
            true
        }
        Err(err) => {
            log_error!(
                "SETTINGS",
                "Could not persist settings to {}: {err}",
                path.display()
            );
            eprintln!("Could not persist settings to {}: {err}", path.display());
            false
        }
    }
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map_or_else(|| OsString::from(SETTINGS_FILE_NAME), OsStr::to_os_string);
    name.push(TEMP_SUFFIX);
    path.with_file_name(name)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Caminho do `settings.json` a partir da base de configuração do usuário.
#[must_use]
pub fn settings_path_from(config_base: Option<PathBuf>, temp_dir: &Path) -> PathBuf {
    config_base.map_or_else(
        || temp_dir.join(FALLBACK_FILE_NAME),
        |base| base.join(SETTINGS_DIR_NAME).join(SETTINGS_FILE_NAME),
    )
}

#[cfg(windows)]
fn config_base() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(PathBuf::from)
}

#[cfg(target_os = "macos")]
fn config_base() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
    })
}

#[cfg(all(unix, not(target_os = "macos")))]
fn config_base() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

/// Returns the default settings path.
///
/// Looks in `$XDG_CONFIG_HOME/clearcore/settings.json` (or `~/.config/...`), `~/Library/Application Support/...`
/// on macOS, `%APPDATA%\clearcore\settings.json` on Windows; without a base, a file in the temp directory.
#[must_use]
pub fn default_settings_path() -> PathBuf {
    settings_path_from(config_base(), &std::env::temp_dir())
}

/// Maps noise suppression intensity percentage (0–100%) to `DeepFilterNet3` `post_filter_beta`.
///
/// Mappings:
/// - 0%   = 0.00 (Minimal / Natural)
/// - 50%  = 0.02 (Standard balanced default)
/// - 75%  = 0.05 (Aggressive)
/// - 100% = 0.10 (Maximum)
#[must_use]
pub fn intensity_to_post_filter_beta(intensity: u8) -> f32 {
    let intensity = intensity.min(100);
    match intensity {
        0 => 0.0,
        1..=50 => (f32::from(intensity) / 50.0) * 0.02,
        51..=75 => (f32::from(intensity - 50) / 25.0).mul_add(0.03, 0.02),
        76..=100 => (f32::from(intensity - 75) / 25.0).mul_add(0.05, 0.05),
        _ => 0.02,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intensity_to_post_filter_beta() {
        assert_eq!(intensity_to_post_filter_beta(0), 0.0);
        assert!((intensity_to_post_filter_beta(50) - 0.02).abs() < f32::EPSILON);
        assert!((intensity_to_post_filter_beta(75) - 0.05).abs() < f32::EPSILON);
        assert!((intensity_to_post_filter_beta(100) - 0.10).abs() < f32::EPSILON);
        assert!((intensity_to_post_filter_beta(150) - 0.10).abs() < f32::EPSILON);
    }
}
