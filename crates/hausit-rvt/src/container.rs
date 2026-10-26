//! Stage 1: the OLE2 / Compound File Binary container.

use std::io::{Read, Seek};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamInfo {
    /// Path inside the container, e.g. `Global/Latest`.
    pub path: String,
    pub size: u64,
}

pub fn list_streams<F: Read + Seek>(cf: &cfb::CompoundFile<F>) -> Vec<StreamInfo> {
    cf.walk()
        .filter(|e| e.is_stream())
        .map(|e| StreamInfo {
            path: e.path().to_string_lossy().trim_start_matches(['/', '\\']).replace('\\', "/"),
            size: e.len(),
        })
        .collect()
}

pub fn read_stream<F: Read + Seek>(cf: &mut cfb::CompoundFile<F>, path: &str) -> std::io::Result<Vec<u8>> {
    let mut s = cf.open_stream(path)?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf)?;
    Ok(buf)
}
