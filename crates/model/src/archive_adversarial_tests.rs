use std::io::{Cursor, Read, Write};

use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use tar::{Archive, Builder, EntryType, Header};

use crate::{InferenceError, archive};

#[derive(Clone)]
struct Member {
    path: String,
    bytes: Vec<u8>,
    mode: u32,
    entry_type: EntryType,
}

fn approved_archive() -> Vec<u8> {
    include_bytes!("../../../vendor/approved/df-compatible-release-asset-v1.bin").to_vec()
}

fn members() -> Result<Vec<Member>, Box<dyn std::error::Error>> {
    let approved = approved_archive();
    let mut archive = Archive::new(GzDecoder::new(approved.as_slice()));
    archive
        .entries()?
        .map(|entry| {
            let mut entry = entry?;
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            Ok(Member {
                path: entry.path()?.into_owned().to_string_lossy().into_owned(),
                bytes,
                mode: entry.header().mode()?,
                entry_type: entry.header().entry_type(),
            })
        })
        .collect()
}

fn encode(members: &[Member]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut builder = Builder::new(encoder);
    for member in members {
        let mut header = Header::new_gnu();
        header.set_size(u64::try_from(member.bytes.len())?);
        header.set_mode(member.mode);
        header.set_entry_type(member.entry_type);
        header.set_cksum();
        builder.append_data(&mut header, &member.path, Cursor::new(&member.bytes))?;
    }
    Ok(builder.into_inner()?.finish()?)
}

fn replace_first_file_path(path: &[u8]) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut tar = Vec::new();
    GzDecoder::new(approved_archive().as_slice()).read_to_end(&mut tar)?;
    // The first actual file entry (enc.onnx) header is at offset 1024 in the raw tar
    // (offset 0 is the PAX extended header)
    const FIRST_FILE_HEADER_OFFSET: usize = 1024;
    tar[FIRST_FILE_HEADER_OFFSET..FIRST_FILE_HEADER_OFFSET + 100].fill(0);
    tar[FIRST_FILE_HEADER_OFFSET..FIRST_FILE_HEADER_OFFSET + path.len()].copy_from_slice(path);
    tar[FIRST_FILE_HEADER_OFFSET + 148..FIRST_FILE_HEADER_OFFSET + 156].fill(b' ');
    let checksum = tar[FIRST_FILE_HEADER_OFFSET..FIRST_FILE_HEADER_OFFSET + 512]
        .iter()
        .map(|byte| u64::from(*byte))
        .sum::<u64>();
    let field = format!("{checksum:06o}\0 ");
    tar[FIRST_FILE_HEADER_OFFSET + 148..FIRST_FILE_HEADER_OFFSET + 156]
        .copy_from_slice(field.as_bytes());
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(&tar)?;
    Ok(encoder.finish()?)
}

fn rejection(bytes: &[u8]) -> Result<String, Box<dyn std::error::Error>> {
    match archive::validate(bytes) {
        Err(InferenceError::ArchiveValidation(message)) => Ok(message),
        Err(error) => Err(format!("wrong error variant: {error}").into()),
        Ok(()) => Err("archive mutation was accepted".into()),
    }
}

#[test]
fn rejects_corrupt_or_truncated_compression_and_tar_streams()
-> Result<(), Box<dyn std::error::Error>> {
    let approved = approved_archive();
    assert!(!rejection(&approved[..approved.len() / 2])?.is_empty());
    assert!(!rejection(b"not-a-gzip-stream")?.is_empty());

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(b"not-a-tar-stream")?;
    assert!(!rejection(&encoder.finish()?)?.is_empty());
    Ok(())
}

#[test]
fn rejects_missing_unexpected_and_duplicate_members() -> Result<(), Box<dyn std::error::Error>> {
    let approved = members()?;
    assert!(rejection(&encode(&approved[1..])?)?.contains("missing required"));

    let mut unexpected = approved.clone();
    unexpected.insert(
        0,
        Member {
            path: "unexpected.bin".to_owned(),
            bytes: Vec::new(),
            mode: 0o644,
            entry_type: EntryType::Regular,
        },
    );
    assert!(rejection(&encode(&unexpected)?)?.contains("unexpected member"));

    let mut duplicate = approved.clone();
    duplicate.insert(1, approved[0].clone());
    assert!(rejection(&encode(&duplicate)?)?.contains("duplicate member"));
    Ok(())
}

#[test]
fn rejects_unsafe_member_paths_and_types() -> Result<(), Box<dyn std::error::Error>> {
    assert!(rejection(&replace_first_file_path(b"../escape")?)?.contains("unexpected member"));
    assert!(rejection(&replace_first_file_path(b"/absolute")?)?.contains("unexpected member"));

    for entry_type in [
        EntryType::Symlink,
        EntryType::Link,
        EntryType::Char,
        EntryType::Block,
    ] {
        let mut changed = members()?;
        changed[0].entry_type = entry_type;
        changed[0].bytes.clear();
        let rejection = rejection(&encode(&changed)?);
        assert!(rejection.is_ok(), "{entry_type:?}: {rejection:?}");
        assert!(rejection?.contains("non-regular member"));
    }
    Ok(())
}

#[test]
fn rejects_executable_size_and_checksum_mutations() -> Result<(), Box<dyn std::error::Error>> {
    let mut executable = members()?;
    executable[0].mode = 0o755;
    assert!(rejection(&encode(&executable)?)?.contains("executable member"));

    let mut wrong_size = members()?;
    wrong_size[0].bytes.pop();
    assert!(rejection(&encode(&wrong_size)?)?.contains("size does not match"));

    let mut wrong_checksum = members()?;
    wrong_checksum[0].bytes[0] ^= 1;
    assert!(rejection(&encode(&wrong_checksum)?)?.contains("checksum does not match"));
    Ok(())
}
