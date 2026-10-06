#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

//! Configuração persistida do serviço (`settings.json`).
//!
//! Leitura tolerante (qualquer problema vira os padrões) e escrita atômica
//! (arquivo temporário + `rename`), com permissão 0600 em unix.

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            preset: Preset::Off,
            backend: None,
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
            },
            _ => Self::default(),
        }
    }

    /// Lê o arquivo; ausente, ilegível ou inválido resulta nos padrões.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse_or_default(&text),
            Err(err) => {
                if err.kind() != io::ErrorKind::NotFound {
                    eprintln!(
                        "Could not read settings file {}: {err}; using defaults",
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
        Ok(()) => true,
        Err(err) => {
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
