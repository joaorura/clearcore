//! Owned by task S3; see docs/superpowers/plans/2026-10-05-voice-enrollment-pipeline-service.md

use crate::voice_samples::{SAMPLES_FILE_NAME, VoiceSampleManager, is_valid_sample_id};
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub migrated_samples: usize,
    pub marked_needs_reenroll: usize,
    pub skipped_invalid_ids: usize,
}

/// Fixed-variant error: never carries free text from files or the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MigrationError {
    Legacy,
    TargetUnreadable,
    Io,
}

impl fmt::Display for MigrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Legacy => "legacy voice samples could not be read",
            Self::TargetUnreadable => "target voice samples could not be read",
            Self::Io => "I/O error during voice samples migration",
        })
    }
}

impl std::error::Error for MigrationError {}

pub const MIGRATION_MARKER_NAME: &str = ".legacy_migrated";

/// One-shot migration of legacy samples (without audio or embedding) into `target_dir`.
///
/// No-op when the legacy manifest is absent or the marker already exists in `target_dir`.
/// The target is loaded strictly: if it exists but cannot be loaded, nothing is written.
/// Legacy files are never modified or deleted.
pub fn migrate_legacy_into_dir(
    legacy_dir: &Path,
    target_dir: &Path,
) -> Result<MigrationReport, MigrationError> {
    let mut report = MigrationReport::default();
    if target_dir.join(MIGRATION_MARKER_NAME).exists()
        || !legacy_dir.join(SAMPLES_FILE_NAME).is_file()
    {
        return Ok(report);
    }
    let mut target =
        VoiceSampleManager::load(target_dir).map_err(|_| MigrationError::TargetUnreadable)?;
    let legacy = VoiceSampleManager::load(legacy_dir).map_err(|_| MigrationError::Legacy)?;
    for sample in legacy.list_samples() {
        if !is_valid_sample_id(&sample.id) {
            report.skipped_invalid_ids += 1;
            continue;
        }
        if target.get_sample(&sample.id).is_some() {
            continue;
        }
        let mut migrated = sample.clone();
        migrated.audio_path = None;
        migrated.speech_seconds = 0.0;
        migrated.embedding.clear();
        target
            .add_sample(migrated)
            .map_err(|_| MigrationError::Io)?;
        report.migrated_samples += 1;
        report.marked_needs_reenroll += 1;
    }
    write_marker(target_dir).map_err(|_| MigrationError::Io)?;
    Ok(report)
}

fn write_marker(target_dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(target_dir)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(target_dir.join(MIGRATION_MARKER_NAME))?
        .sync_all()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::voice_samples::{VOICE_EMBEDDING_DIM, VoiceSample};

    fn legacy_with_two(dir: &Path) {
        let mut legacy = VoiceSampleManager::new(dir);
        for (id, idx) in [("a", 0usize), ("b", 1)] {
            let mut e = vec![0.0f32; VOICE_EMBEDDING_DIM];
            e[idx] = 1.0;
            let mut s = VoiceSample::new(id, "1", id, Some("blob:x".into()), e).unwrap();
            s.speech_seconds = 3.0;
            legacy.add_sample(s).unwrap();
        }
    }

    fn td(root: &Path) -> std::path::PathBuf {
        root.join("new")
    }

    #[test]
    fn migrates_two_samples_without_audio_embedding_or_profile_bin() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("legacy");
        legacy_with_two(&legacy_dir);
        let before = fs::read(legacy_dir.join("voice_samples.json")).unwrap();
        let r = migrate_legacy_into_dir(&legacy_dir, &td(temp.path())).unwrap();
        assert_eq!(r.migrated_samples, 2);
        assert_eq!(r.marked_needs_reenroll, 2);
        assert_eq!(r.skipped_invalid_ids, 0);
        let target = VoiceSampleManager::load(td(temp.path())).unwrap();
        assert_eq!(target.list_samples().len(), 2);
        for s in target.list_samples() {
            assert!(s.audio_path.is_none());
            assert!(s.speech_seconds.abs() < f32::EPSILON);
            assert!(s.embedding.is_empty());
        }
        assert!(!td(temp.path()).join("profile.bin").exists());
        assert_eq!(
            fs::read(legacy_dir.join("voice_samples.json")).unwrap(),
            before
        );
        let again = migrate_legacy_into_dir(&legacy_dir, &td(temp.path())).unwrap();
        assert_eq!(again, MigrationReport::default());
    }

    #[cfg(unix)]
    #[test]
    fn marker_is_empty_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("legacy");
        legacy_with_two(&legacy_dir);
        migrate_legacy_into_dir(&legacy_dir, &td(temp.path())).unwrap();
        let marker = td(temp.path()).join(".legacy_migrated");
        let meta = fs::metadata(&marker).unwrap();
        assert_eq!(meta.len(), 0);
        assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn deleted_sample_does_not_come_back() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("legacy");
        legacy_with_two(&legacy_dir);
        migrate_legacy_into_dir(&legacy_dir, &td(temp.path())).unwrap();
        let mut target = VoiceSampleManager::load(td(temp.path())).unwrap();
        assert!(target.delete_sample("a", true).unwrap());
        let reloaded = VoiceSampleManager::load(td(temp.path())).unwrap();
        let r = migrate_legacy_into_dir(&legacy_dir, &td(temp.path())).unwrap();
        assert_eq!(r, MigrationReport::default());
        let after = VoiceSampleManager::load(td(temp.path())).unwrap();
        assert_eq!(reloaded.list_samples().len(), 1);
        assert!(after.get_sample("a").is_none());
        assert_eq!(after.list_samples().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_target_is_an_error_and_left_byte_identical() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("legacy");
        legacy_with_two(&legacy_dir);
        for (mode, body) in [
            (0o644, br#"[]"#.as_slice()),
            (0o600, b"{not json".as_slice()),
        ] {
            let dir = temp.path().join(format!("t{mode:o}"));
            fs::create_dir_all(&dir).unwrap();
            let file = dir.join("voice_samples.json");
            fs::write(&file, body).unwrap();
            fs::set_permissions(&file, fs::Permissions::from_mode(mode)).unwrap();
            let err = migrate_legacy_into_dir(&legacy_dir, &dir).unwrap_err();
            assert_eq!(err, MigrationError::TargetUnreadable);
            assert_eq!(fs::read(&file).unwrap(), body);
            assert!(!dir.join(".legacy_migrated").exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn invalid_legacy_ids_are_skipped_and_counted() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("legacy");
        fs::create_dir_all(&legacy_dir).unwrap();
        let file = legacy_dir.join("voice_samples.json");
        fs::write(
            &file,
            br#"[{"id":"../x","timestamp":"1","name":"n","audio_path":null},{"id":"ok","timestamp":"1","name":"n","audio_path":null}]"#,
        )
        .unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
        let r = migrate_legacy_into_dir(&legacy_dir, &td(temp.path())).unwrap();
        assert_eq!(r.migrated_samples, 1);
        assert_eq!(r.skipped_invalid_ids, 1);
        let t = VoiceSampleManager::load(td(temp.path())).unwrap();
        assert!(t.get_sample("ok").is_some() && t.list_samples().len() == 1);
    }

    #[test]
    fn no_legacy_is_a_noop() {
        let temp = tempfile::tempdir().unwrap();
        let r = migrate_legacy_into_dir(&temp.path().join("nope"), &td(temp.path())).unwrap();
        assert_eq!(r, MigrationReport::default());
        assert!(!td(temp.path()).exists());
    }

    #[test]
    fn migration_error_display_is_fixed() {
        assert_eq!(
            MigrationError::Legacy.to_string(),
            "legacy voice samples could not be read"
        );
    }
}
