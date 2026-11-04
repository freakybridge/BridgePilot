//! Deterministic ustar archive; only the explicitly selected, validated files enter it.
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::Path,
};

fn field(header: &mut [u8], value: u64) -> Result<(), String> {
    let s = format!("{:0width$o}\0", value, width = header.len() - 1);
    if s.len() != header.len() {
        return Err("release archive field overflow".into());
    }
    header.copy_from_slice(s.as_bytes());
    Ok(())
}
pub(super) fn write(
    root: &Path,
    paths: &[String],
    expected: &BTreeMap<String, String>,
    output: &Path,
) -> Result<(), String> {
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    for rel in paths {
        let file = bridgeforge_core::managed_paths::safe_join(root, rel, "release archive input")?;
        let metadata = fs::symlink_metadata(&file).map_err(|e| e.to_string())?;
        if !metadata.is_file()
            || bridgeforge_core::persistence::is_link_or_reparse(&file)
                .map_err(|e| e.to_string())?
        {
            return Err("archive input must be a regular file".into());
        }
        let (prefix, name) = if rel.len() <= 100 {
            ("", rel.as_str())
        } else {
            let at = rel
                .match_indices('/')
                .map(|(i, _)| i)
                .filter(|i| *i <= 155 && rel.len() - i - 1 <= 100)
                .next_back()
                .ok_or("release archive path exceeds ustar limit")?;
            (&rel[..at], &rel[at + 1..])
        };
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        field(&mut h[100..108], 0o644)?;
        field(&mut h[108..116], 0)?;
        field(&mut h[116..124], 0)?;
        field(&mut h[124..136], metadata.len())?;
        field(&mut h[136..148], 0)?;
        h[148..156].fill(b' ');
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        let checksum = h.iter().map(|b| *b as u64).sum::<u64>();
        let sum = format!("{checksum:06o}\0 ");
        h[148..156].copy_from_slice(sum.as_bytes());
        out.write_all(&h).map_err(|e| e.to_string())?;
        let mut input = fs::File::open(&file).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut copied = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            copied += n as u64;
            if copied > metadata.len() {
                return Err("archive input length changed".into());
            }
            hash.update(&buffer[..n]);
            out.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        }
        if copied != metadata.len() {
            return Err("archive input length changed".into());
        }
        if expected.get(rel) != Some(&format!("sha256:{:x}", hash.finalize())) {
            return Err(format!("archive input content changed: {rel}"));
        }
        let pad = (512 - (copied % 512)) % 512;
        out.write_all(&vec![0u8; pad as usize])
            .map_err(|e| e.to_string())?;
    }
    out.write_all(&[0u8; 1024])
        .and_then(|_| out.sync_all())
        .map_err(|e| e.to_string())
}
