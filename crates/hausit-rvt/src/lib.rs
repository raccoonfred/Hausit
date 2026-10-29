//! Reader for Autodesk Revit files (`.rvt`, `.rfa`, `.rte`, `.rft`).
//!
//! # Status
//!
//! Revit files are OLE2 / Compound File Binary containers whose element data
//! is stored in an undocumented, version-specific binary format. Reading
//! them fully is a long-term research milestone. This crate is the boundary
//! where that work lives, organised as a pipeline:
//!
//! | Stage | What it does | Status |
//! |-------|--------------|--------|
//! | 1. [`container`] | Open the compound file, list its streams | **done** |
//! | 2. [`basic_info`] | Read `BasicFileInfo`: Revit version, build, worksharing | **done** |
//! | 3. [`preview`] | Extract the PNG thumbnail from `RevitPreview4.0` | **done** |
//! | 4. [`partitions`] | Inflate the gzip data in `Formats`, `Global/*` and `Partitions/*` | **done** |
//! | 5. [`schema`] | Read the class names, parents and field names in `Formats/Latest` | **partly done**: names and hierarchy; field types not decoded |
//! | 6. Elements | Decode element records into typed data | not started |
//! | 7. Mapping | Convert decoded elements into a [`hausit_model::Document`] | not started |
//!
//! [`import`] performs every implemented stage and then returns
//! [`RvtError::ElementDataNotSupported`] with what it learned. It never
//! returns a made-up model.

pub mod basic_info;
pub mod container;
pub mod partitions;
pub mod preview;
pub mod schema;

use std::io::{Read, Seek};
use std::path::Path;
use thiserror::Error;

pub use basic_info::BasicFileInfo;
pub use container::StreamInfo;

/// Kind of Revit file, from its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevitFileKind {
    Project,
    Family,
    ProjectTemplate,
    FamilyTemplate,
    Unknown,
}

impl RevitFileKind {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "rvt" => Self::Project,
            "rfa" => Self::Family,
            "rte" => Self::ProjectTemplate,
            "rft" => Self::FamilyTemplate,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "Revit project (.rvt)",
            Self::Family => "Revit family (.rfa)",
            Self::ProjectTemplate => "Revit project template (.rte)",
            Self::FamilyTemplate => "Revit family template (.rft)",
            Self::Unknown => "Revit file",
        }
    }
}

/// File extensions this crate recognises.
pub const EXTENSIONS: [&str; 4] = ["rvt", "rfa", "rte", "rft"];

/// Everything that can currently be learned from a Revit file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RevitFileInfo {
    pub kind: Option<RevitFileKind>,
    pub basic: Option<BasicFileInfo>,
    pub streams: Vec<StreamInfo>,
    pub preview_png: Option<Vec<u8>>,
    /// Compressed streams: path, stored size, gzip members, inflated size.
    pub data_streams: Vec<DataStream>,
    pub schema: Option<schema::Schema>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataStream {
    pub path: String,
    pub stored: usize,
    pub members: usize,
    pub inflated: usize,
}

impl RevitFileInfo {
    /// Human readable summary, used by the UI.
    pub fn summary(&self) -> String {
        let mut s = String::new();
        if let Some(k) = self.kind {
            s += &format!("Type: {}\n", k.label());
        }
        match &self.basic {
            Some(b) => {
                s += &format!("Saved in: {}\n", b.version_label());
                if let Some(build) = &b.build {
                    s += &format!("Build: {build}\n");
                }
                if let Some(w) = &b.worksharing {
                    s += &format!("Worksharing: {w}\n");
                }
                if let Some(p) = &b.last_save_path {
                    s += &format!("Last saved to: {p}\n");
                }
            }
            None => s += "Version: unknown (BasicFileInfo not readable)\n",
        }
        s += &format!("Streams in container: {}\n", self.streams.len());
        if !self.data_streams.is_empty() {
            let stored: usize = self.data_streams.iter().map(|d| d.stored).sum();
            let inflated: usize = self.data_streams.iter().map(|d| d.inflated).sum();
            s += &format!(
                "Compressed data: {} streams, {:.1} MB stored, {:.1} MB decompressed\n",
                self.data_streams.len(),
                stored as f64 / 1e6,
                inflated as f64 / 1e6
            );
        }
        if let Some(schema) = &self.schema {
            let names = schema.class_names();
            let known: Vec<&str> = [
                "VWall",
                "ArcWall",
                "WallType",
                "FloorType",
                "Level",
                "Grid",
                "FamilyInstance",
                "Room",
            ]
            .into_iter()
            .filter(|n| names.contains(n))
            .collect();
            s += &format!("Schema: {} classes", names.len());
            if !known.is_empty() {
                s += &format!(" (including {})", known.join(", "));
            }
            s += "\n";
        }
        s
    }
}

#[derive(Debug, Error)]
pub enum RvtError {
    #[error("could not read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("not a Revit file: it is not an OLE compound file")]
    NotCompoundFile,
    #[error(
        "Hausit can read this file's header but cannot import Revit elements yet. \
         Importing .rvt/.rfa content is a planned research milestone."
    )]
    ElementDataNotSupported(Box<RevitFileInfo>),
}

/// Reads all metadata that is currently supported.
pub fn inspect_reader<R: Read + Seek>(reader: R, kind: Option<RevitFileKind>) -> Result<RevitFileInfo, RvtError> {
    let mut cf = cfb::CompoundFile::open(reader).map_err(|e| {
        if e.kind() == std::io::ErrorKind::InvalidData {
            RvtError::NotCompoundFile
        } else {
            RvtError::Io(e)
        }
    })?;
    let streams = container::list_streams(&cf);
    let basic = container::read_stream(&mut cf, "BasicFileInfo")
        .ok()
        .and_then(|b| basic_info::parse(&b));
    let preview_png = container::read_stream(&mut cf, "RevitPreview4.0")
        .ok()
        .and_then(|b| preview::extract_png(&b));
    let mut data_streams = Vec::new();
    let mut schema = None;
    for st in &streams {
        let compressed = st.path.starts_with("Global/") || st.path.starts_with("Partitions/") || st.path.starts_with("Formats/");
        if !compressed {
            continue;
        }
        let Ok(raw) = container::read_stream(&mut cf, &st.path) else {
            continue;
        };
        let inflated = partitions::inflate_stream(&raw);
        if st.path == "Formats/Latest" && !inflated.data.is_empty() {
            schema = Some(schema::parse(&inflated.data));
        }
        data_streams.push(DataStream {
            path: st.path.clone(),
            stored: raw.len(),
            members: inflated.members.len(),
            inflated: inflated.data.len(),
        });
    }
    Ok(RevitFileInfo {
        kind,
        basic,
        streams,
        preview_png,
        data_streams,
        schema,
    })
}

pub fn inspect(path: &Path) -> Result<RevitFileInfo, RvtError> {
    let kind = path.extension().and_then(|e| e.to_str()).map(RevitFileKind::from_extension);
    let f = std::fs::File::open(path)?;
    inspect_reader(std::io::BufReader::new(f), kind)
}

/// Imports a Revit file into a Hausit document.
///
/// Currently always fails with [`RvtError::ElementDataNotSupported`] after
/// running the implemented stages, so callers can show the file's version
/// and preview to the user.
pub fn import(path: &Path) -> Result<hausit_model::Document, RvtError> {
    let info = inspect(path)?;
    Err(RvtError::ElementDataNotSupported(Box::new(info)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    /// Builds a compound file shaped like a Revit file. This is a synthetic
    /// fixture, not real Revit output.
    fn fixture(basic: &[u8]) -> Vec<u8> {
        let mut cf = cfb::CompoundFile::create(Cursor::new(Vec::new())).unwrap();
        cf.create_stream("BasicFileInfo").unwrap().write_all(basic).unwrap();
        let mut preview = vec![0u8; 16];
        preview.extend_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 1, 2, 3]);
        preview.extend_from_slice(b"IEND\xAE\x42\x60\x82trailing");
        cf.create_stream("RevitPreview4.0").unwrap().write_all(&preview).unwrap();
        cf.create_storage("Global").unwrap();
        cf.create_stream("Global/Latest").unwrap().write_all(&[0; 64]).unwrap();
        cf.flush().unwrap();
        cf.into_inner().into_inner()
    }

    #[test]
    fn reads_modern_header() {
        let mut basic = vec![4, 0, 0, 0, 0, 0];
        basic.extend(utf16("Worksharing: Not enabled\r\nUsername: \r\nFormat: 2024\r\nBuild: 20230308_1635(x64)\r\nLast Save Path: C:\\proj\\house.rvt\r\n"));
        let info = inspect_reader(Cursor::new(fixture(&basic)), Some(RevitFileKind::Project)).unwrap();
        let b = info.basic.unwrap();
        assert_eq!(b.version, Some(2024));
        assert_eq!(b.build.as_deref(), Some("20230308_1635(x64)"));
        assert_eq!(b.worksharing.as_deref(), Some("Not enabled"));
        assert!(info.streams.iter().any(|s| s.path == "Global/Latest"));
        let png = info.preview_png.unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(png.ends_with(b"IEND\xAE\x42\x60\x82"));
    }

    #[test]
    fn reads_legacy_header() {
        let mut basic = vec![1, 0, 0, 0];
        basic.extend(utf16(
            "Worksharing: Not enabled\r\nRevit Build: Autodesk Revit 2016 (Build: 20150220_1215(x64))\r\n",
        ));
        let info = inspect_reader(Cursor::new(fixture(&basic)), None).unwrap();
        let b = info.basic.unwrap();
        assert_eq!(b.version, Some(2016));
        assert_eq!(b.product.as_deref(), Some("Autodesk Revit 2016"));
        assert_eq!(b.build.as_deref(), Some("20150220_1215(x64)"));
    }

    #[test]
    fn rejects_non_compound_files() {
        let err = inspect_reader(Cursor::new(b"not a revit file at all, just text....".repeat(20)), None).unwrap_err();
        assert!(matches!(err, RvtError::NotCompoundFile | RvtError::Io(_)));
    }
}
