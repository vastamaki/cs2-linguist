use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Model {
    pub id: &'static str,
    pub filename: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub url: &'static str,
}

// SHA-256 and byte lengths come from the upstream Git LFS pointers at these pinned revisions.
pub const MODELS: [Model; 5] = [
    Model { id: "base", filename: "ggml-base.bin", bytes: 147951465,
        sha256: "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin" },
    Model { id: "small", filename: "ggml-small.bin", bytes: 487601967,
        sha256: "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.bin" },
    Model { id: "medium", filename: "ggml-medium.bin", bytes: 1533763059,
        sha256: "6c14d5adee5f86394037b4e4e8b59f1673b6cee10e3cf0b11bbdbee79c156208",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-medium.bin" },
    Model { id: "large-v3", filename: "ggml-large-v3.bin", bytes: 3095033483,
        sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-large-v3.bin" },
    Model { id: "vad", filename: "ggml-silero-v6.2.0.bin", bytes: 885098,
        sha256: "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987",
        url: "https://huggingface.co/ggml-org/whisper-vad/resolve/9ffd54a1e1ee413ddf265af9913beaf518d1639b/ggml-silero-v6.2.0.bin" },
];

pub fn model(id: &str) -> Result<&'static Model, String> {
    MODELS
        .iter()
        .find(|m| m.id == id)
        .ok_or_else(|| "Unknown model.".into())
}

pub fn verify_reader(
    mut reader: impl Read,
    expected_bytes: u64,
    expected_hash: &str,
) -> io::Result<()> {
    let mut hash = Sha256::new();
    let mut bytes = 0;
    let mut buffer = [0u8; 65536];
    loop {
        let n = reader.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > expected_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Model is larger than expected.",
            ));
        }
        hash.update(&buffer[..n]);
    }
    if bytes != expected_bytes || format!("{:x}", hash.finalize()) != expected_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Model checksum mismatch. Download or import the supported model again.",
        ));
    }
    Ok(())
}

pub fn verify_file(path: &Path, model: &Model) -> io::Result<()> {
    verify_reader(File::open(path)?, model.bytes, model.sha256)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integrity_rejects_corrupt_truncated_and_oversize_files() {
        let hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_reader(&b"abc"[..], 3, hash).is_ok());
        for invalid in [&b"abd"[..], &b"ab"[..], &b"abcd"[..]] {
            assert!(verify_reader(invalid, 3, hash).is_err());
        }
    }
}
