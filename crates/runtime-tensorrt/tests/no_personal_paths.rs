#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]

//! Library discovery must come from the environment or the system loader, never from a path
//! baked in by whoever wrote the code.

use std::fs;
use std::path::Path;

fn collect_rs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn sources_embed_no_home_directory_paths() {
    // Built with concat! so this file does not trip its own check.
    let needle = concat!("/ho", "me/");
    let mut files = Vec::new();
    collect_rs(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(!files.is_empty());
    let offenders: Vec<_> = files
        .iter()
        .filter(|path| fs::read_to_string(path).unwrap().contains(needle))
        .collect();
    assert!(offenders.is_empty(), "personal paths in {offenders:?}");
}
