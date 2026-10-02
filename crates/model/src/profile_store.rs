//! On-disk store for the active voice profile.
//!
//! Voice profiles are biometric data. The store keeps a single profile at
//! `<dir>/active_profile.json`, with the directory at `0700` and the file at `0600` (POSIX).
//! Nothing here is ever read by the diagnostics exporter.

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::voice_profile::{VoiceProfile, VoiceProfileError};

pub const ACTIVE_PROFILE_FILE_NAME: &str = "active_profile.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileStore {
    dir: PathBuf,
}

impl ProfileStore {
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Default location, `<data dir>/clearcore/profiles`, resolved from the environment:
    /// `XDG_DATA_HOME` or `~/.local/share` on Linux, `~/Library/Application Support` on macOS and
    /// `%APPDATA%` on Windows. Returns `None` when no base directory can be determined.
    #[must_use]
    pub fn default_dir() -> Option<PathBuf> {
        let base = if cfg!(windows) {
            std::env::var_os("APPDATA").map(PathBuf::from)
        } else if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("Library/Application Support"))
        } else {
            std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
                })
        };
        base.map(|base| base.join("clearcore").join("profiles"))
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    #[must_use]
    pub fn active_profile_path(&self) -> PathBuf {
        self.dir.join(ACTIVE_PROFILE_FILE_NAME)
    }

    /// Refuses a profile directory that is itself a symlink. `create_dir_all`, `metadata` and
    /// `set_permissions` all follow it, so the biometric profile could be read from, written to
    /// or `chmod`ed in a directory chosen by whoever planted the link.
    fn refuse_symlinked_dir(&self) -> Result<(), VoiceProfileError> {
        match fs::symlink_metadata(&self.dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(VoiceProfileError::InsecurePermissions(format!(
                    "profile directory {} is a symlink",
                    self.dir.display()
                )))
            }
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn ensure_dir(&self) -> Result<(), VoiceProfileError> {
        self.refuse_symlinked_dir()?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&self.dir)?;
        // `DirBuilder::mode` only applies to directories it creates (and is masked by the umask).
        // A directory that already existed with a looser mode (e.g. made by hand or by an older
        // build) holds biometric data, so tighten it to owner-only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&self.dir)?.permissions().mode();
            if mode & 0o077 != 0 {
                fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700))?;
            }
        }
        Ok(())
    }

    /// Atomically persists `profile` as the active profile (file mode `0600`).
    pub fn save_active(&self, profile: &VoiceProfile) -> Result<(), VoiceProfileError> {
        self.ensure_dir()?;
        profile.save_to_file_secure(&self.active_profile_path())
    }

    /// Loads the active profile. `Ok(None)` when none is stored; a stored profile with
    /// insecure permissions, a bad integrity hash or an invalid payload is an error
    /// (fail-closed: it is never silently ignored or loaded).
    pub fn load_active(&self) -> Result<Option<VoiceProfile>, VoiceProfileError> {
        self.refuse_symlinked_dir()?;
        let path = self.active_profile_path();
        match fs::symlink_metadata(&path) {
            Ok(_) => VoiceProfile::load_from_file_secure(&path).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Removes the active profile. Idempotent: removing nothing is not an error.
    pub fn clear_active(&self) -> Result<(), VoiceProfileError> {
        self.refuse_symlinked_dir()?;
        match fs::remove_file(self.active_profile_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn profile(id: &str) -> VoiceProfile {
        VoiceProfile::identity(id, "Speaker", "2026-10-02T12:00:00Z").expect("profile")
    }

    #[test]
    fn load_without_profile_is_none() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ProfileStore::new(temp.path().join("profiles"));
        assert_eq!(store.load_active().expect("load"), None);
    }

    #[test]
    fn save_then_load_roundtrips_and_replaces() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ProfileStore::new(temp.path().join("nested/profiles"));
        store.save_active(&profile("spk-1")).expect("save 1");
        assert_eq!(
            store.load_active().expect("load").expect("some").id,
            "spk-1"
        );
        store.save_active(&profile("spk-2")).expect("save 2");
        assert_eq!(
            store.load_active().expect("load").expect("some").id,
            "spk-2"
        );
    }

    #[cfg(unix)]
    #[test]
    fn files_are_0600_and_directory_is_0700() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ProfileStore::new(temp.path().join("profiles"));
        store.save_active(&profile("spk-1")).expect("save");
        let file_mode = fs::metadata(store.active_profile_path())
            .expect("file")
            .permissions()
            .mode();
        let dir_mode = fs::metadata(store.dir()).expect("dir").permissions().mode();
        assert_eq!(file_mode & 0o777, 0o600);
        assert_eq!(dir_mode & 0o777, 0o700);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_profile_directory_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("elsewhere");
        fs::create_dir(&target).expect("mkdir");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).expect("chmod");
        let link = temp.path().join("profiles");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        let store = ProfileStore::new(&link);

        for result in [
            store.save_active(&profile("spk-1")),
            store.load_active().map(|_| ()),
            store.clear_active(),
        ] {
            assert!(
                matches!(&result, Err(VoiceProfileError::InsecurePermissions(m)) if m.contains("symlink")),
                "{result:?}"
            );
        }
        // Nothing was written through the link and the target keeps its mode.
        assert_eq!(fs::read_dir(&target).expect("read").count(), 0);
        let mode = fs::metadata(&target).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[cfg(unix)]
    #[test]
    fn save_tightens_a_preexisting_open_directory_to_0700() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("profiles");
        fs::create_dir(&dir).expect("mkdir");
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).expect("chmod");
        let store = ProfileStore::new(&dir);
        store.save_active(&profile("spk-1")).expect("save");
        let dir_mode = fs::metadata(&dir).expect("dir").permissions().mode();
        assert_eq!(dir_mode & 0o777, 0o700);
        assert!(store.load_active().expect("load").is_some());
    }

    #[cfg(unix)]
    #[test]
    fn load_refuses_group_readable_profile() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ProfileStore::new(temp.path().join("profiles"));
        store.save_active(&profile("spk-1")).expect("save");
        fs::set_permissions(
            store.active_profile_path(),
            fs::Permissions::from_mode(0o640),
        )
        .expect("chmod");
        assert!(matches!(
            store.load_active(),
            Err(VoiceProfileError::InsecurePermissions(_))
        ));
    }

    #[test]
    fn load_refuses_tampered_profile() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ProfileStore::new(temp.path().join("profiles"));
        store.save_active(&profile("spk-1")).expect("save");
        let path = store.active_profile_path();
        let tampered = fs::read_to_string(&path)
            .expect("read")
            .replace("Speaker", "Imposter");
        fs::write(&path, tampered).expect("write");
        assert!(matches!(
            store.load_active(),
            Err(VoiceProfileError::IntegrityMismatch { .. })
        ));
    }

    #[test]
    fn clear_is_idempotent_and_removes_the_profile() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ProfileStore::new(temp.path().join("profiles"));
        store.clear_active().expect("clear when empty");
        store.save_active(&profile("spk-1")).expect("save");
        store.clear_active().expect("clear");
        assert_eq!(store.load_active().expect("load"), None);
        store.clear_active().expect("clear again");
    }

    #[test]
    fn no_temp_files_left_after_save() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ProfileStore::new(temp.path().join("profiles"));
        store.save_active(&profile("spk-1")).expect("save");
        let names: Vec<_> = fs::read_dir(store.dir())
            .expect("read_dir")
            .map(|entry| entry.expect("entry").file_name())
            .collect();
        assert_eq!(
            names,
            vec![std::ffi::OsString::from(ACTIVE_PROFILE_FILE_NAME)]
        );
    }
}
