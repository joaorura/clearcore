use std::{collections::BTreeSet, io::Read};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

use crate::InferenceError;

const MEMBERS: [(&str, u64, &str); 4] = [
    (
        "tmp/export/enc.onnx",
        1_954_042,
        "7c5399d3da8a50ebef1c1a0ae421b33376aa5e45d0e92df16da7e83c9c131916",
    ),
    (
        "tmp/export/erb_dec.onnx",
        3_292_397,
        "ab669a1d10afe20911728b33053a452071042317a90581092b325da7b2f9d895",
    ),
    (
        "tmp/export/df_dec.onnx",
        3_340_803,
        "23114ce3b0f6464b763ee62f7bb8aab6b2a129a21eabd5bcfe59413db05f278a",
    ),
    (
        "tmp/export/config.ini",
        2_067,
        "415eb925d44990d938fb739f514aa3662c1ec0ea836cff044fa1291b82cb4290",
    ),
];

pub fn validate(bytes: &[u8]) -> Result<(), InferenceError> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = Archive::new(decoder);
    let entries = archive.entries().map_err(archive_error)?;
    let mut seen = BTreeSet::new();

    for entry in entries {
        let mut entry = entry.map_err(archive_error)?;
        if !entry.header().entry_type().is_file() {
            return Err(invalid("archive contains a non-regular member"));
        }
        let mode = entry.header().mode().map_err(archive_error)?;
        if mode & 0o111 != 0 {
            return Err(invalid("archive contains an executable member"));
        }
        let path = entry.path().map_err(archive_error)?;
        let path = path
            .to_str()
            .ok_or_else(|| invalid("member path is not UTF-8"))?;
        let (_, expected_size, expected_hash) = MEMBERS
            .iter()
            .find(|(required, _, _)| *required == path)
            .ok_or_else(|| invalid("archive contains an unexpected member"))?;
        if !seen.insert(path.to_owned()) {
            return Err(invalid("archive contains a duplicate member"));
        }
        if entry.size() != *expected_size {
            return Err(invalid("archive member size does not match approval"));
        }
        let capacity = usize::try_from(*expected_size)
            .map_err(|_| invalid("archive member is too large for this platform"))?;
        let mut member = Vec::with_capacity(capacity);
        entry.read_to_end(&mut member).map_err(archive_error)?;
        let digest = format!("{:x}", Sha256::digest(&member));
        if digest != *expected_hash {
            return Err(invalid("archive member checksum does not match approval"));
        }
    }

    if seen.len() != MEMBERS.len() {
        return Err(invalid("archive is missing required members"));
    }
    Ok(())
}

fn archive_error(error: impl std::fmt::Display) -> InferenceError {
    invalid(&error.to_string())
}

fn invalid(message: &str) -> InferenceError {
    InferenceError::ArchiveValidation(message.to_owned())
}
