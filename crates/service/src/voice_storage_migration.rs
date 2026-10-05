//! Owned by task S3; see docs/superpowers/plans/2026-10-05-voice-enrollment-pipeline-service.md
// removed when Task S6 wires the module
#![allow(dead_code)]

use crate::voice_samples::{SAMPLES_FILE_NAME, VoiceSampleManager};
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MigrationReport {
    pub migrated_samples: usize,
    pub marked_needs_reenroll: usize,
}

/// Copies legacy samples (by id, not yet present in `target`) with no audio.
/// Never modifies or deletes the legacy files; idempotent.
pub fn migrate_legacy_samples(
    legacy_dir: &Path,
    target: &mut VoiceSampleManager,
) -> io::Result<MigrationReport> {
    let mut report = MigrationReport::default();
    if !legacy_dir.join(SAMPLES_FILE_NAME).is_file() {
        return Ok(report);
    }
    let legacy = VoiceSampleManager::load(legacy_dir)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    for sample in legacy.list_samples() {
        if target.get_sample(&sample.id).is_some() {
            continue;
        }
        let mut migrated = sample.clone();
        migrated.audio_path = None;
        migrated.speech_seconds = 0.0;
        target
            .add_sample(migrated)
            .map_err(|e| io::Error::other(e.to_string()))?;
        report.migrated_samples += 1;
        report.marked_needs_reenroll += 1;
    }
    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::voice_samples::{VOICE_EMBEDDING_DIM, VoiceSample};
    use std::fs;

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

    #[test]
    fn migrates_two_samples_once_and_is_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        let legacy_dir = temp.path().join("legacy");
        legacy_with_two(&legacy_dir);
        let before = fs::read(legacy_dir.join("voice_samples.json")).unwrap();
        let mut target = VoiceSampleManager::new(temp.path().join("new"));

        let r = migrate_legacy_samples(&legacy_dir, &mut target).unwrap();
        assert_eq!(r.migrated_samples, 2);
        assert_eq!(r.marked_needs_reenroll, 2);
        for s in target.list_samples() {
            assert!(s.audio_path.is_none());
            assert!(s.speech_seconds.abs() < f32::EPSILON);
            assert_eq!(s.embedding.len(), VOICE_EMBEDDING_DIM);
        }
        let again = migrate_legacy_samples(&legacy_dir, &mut target).unwrap();
        assert_eq!(again, MigrationReport::default());
        assert_eq!(target.list_samples().len(), 2);
        assert_eq!(
            fs::read(legacy_dir.join("voice_samples.json")).unwrap(),
            before
        );
        let reloaded = VoiceSampleManager::load(temp.path().join("new")).unwrap();
        assert_eq!(reloaded.list_samples().len(), 2);
    }

    #[test]
    fn no_legacy_is_a_noop() {
        let temp = tempfile::tempdir().unwrap();
        let mut target = VoiceSampleManager::new(temp.path().join("new"));
        let r = migrate_legacy_samples(&temp.path().join("nope"), &mut target).unwrap();
        assert_eq!(r, MigrationReport::default());
        assert!(target.list_samples().is_empty());
        assert!(!temp.path().join("new").exists());
    }
}
