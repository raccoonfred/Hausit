//! Project files: the native `.hausit` format and the import registry.
//!
//! `.hausit` is versioned JSON, readable and diff-friendly. Foreign formats
//! go through [`open`], which dispatches on the extension; Revit files are
//! handled by `hausit-rvt`.

pub mod ifc;
pub mod ifc_import;
pub mod step;

use hausit_model::Document;
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

pub const EXTENSION: &str = "hausit";
pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum IoError {
    #[error("could not access file: {0}")]
    Io(#[from] std::io::Error),
    #[error("file is not a valid Hausit project: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("this project was saved by a newer Hausit (format {0}); please update Hausit")]
    NewerFormat(u32),
    #[error("unsupported file type \".{0}\"")]
    UnsupportedExtension(String),
    #[error("could not read the IFC file: {0}")]
    Ifc(String),
    #[error(transparent)]
    Revit(#[from] hausit_rvt::RvtError),
}

#[derive(Serialize, Deserialize)]
struct Envelope<D> {
    format: String,
    version: u32,
    generator: String,
    document: D,
}

pub fn to_string(doc: &Document) -> Result<String, IoError> {
    let env = Envelope {
        format: "hausit-project".into(),
        version: FORMAT_VERSION,
        generator: format!("Hausit {}", env!("CARGO_PKG_VERSION")),
        document: doc,
    };
    Ok(serde_json::to_string_pretty(&env)?)
}

pub fn from_str(s: &str) -> Result<Document, IoError> {
    // Check the version before the document, so a newer file reports that
    // rather than a confusing parse error.
    let env: Envelope<serde_json::Value> = serde_json::from_str(s)?;
    if env.version > FORMAT_VERSION {
        return Err(IoError::NewerFormat(env.version));
    }
    let mut doc: Document = serde_json::from_value(env.document)?;
    doc.touch();
    Ok(doc)
}

/// Saves atomically: writes a temporary file, then renames it over the
/// target so a crash never leaves a half-written project.
pub fn save(doc: &Document, path: &Path) -> Result<(), IoError> {
    let text = to_string(doc)?;
    let tmp = path.with_extension("hausit.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Document, IoError> {
    from_str(&std::fs::read_to_string(path)?)
}

/// Opens any supported file type.
pub fn open(path: &Path) -> Result<Document, IoError> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        EXTENSION => load(path),
        "ifc" => Ok(import_ifc(path)?.0),
        e if hausit_rvt::EXTENSIONS.contains(&e) => Ok(hausit_rvt::import(path)?),
        other => Err(IoError::UnsupportedExtension(other.to_string())),
    }
}

/// Imports an IFC file (IFC2x3 or IFC4) as a new project, with a report
/// of what came in.
pub fn import_ifc(path: &Path) -> Result<(Document, ifc_import::ImportReport), IoError> {
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let (mut doc, report) = ifc_import::import(&text).map_err(IoError::Ifc)?;
    doc.touch();
    Ok((doc, report))
}

/// Exports the document as IFC4.
pub fn export_ifc(doc: &Document, path: &Path) -> Result<(), IoError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    std::fs::write(path, ifc::export(doc, now))?;
    Ok(())
}

/// Extensions accepted by [`open`], for file dialogs.
pub fn openable_extensions() -> Vec<&'static str> {
    let mut v = vec![EXTENSION, "ifc"];
    v.extend(hausit_rvt::EXTENSIONS);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use hausit_model::template::sample_project;

    #[test]
    fn round_trip() {
        let doc = sample_project();
        let text = to_string(&doc).unwrap();
        let back = from_str(&text).unwrap();
        // Revision is runtime-only; compare the serialised content.
        assert_eq!(to_string(&back).unwrap(), text);
    }

    #[test]
    fn save_and_open() {
        let dir = std::env::temp_dir().join(format!("hausit-io-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("house.hausit");
        save(&sample_project(), &p).unwrap();
        let doc = open(&p).unwrap();
        assert_eq!(doc.project_info.name, "Sample House");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_newer_format() {
        let s = r#"{"format":"hausit-project","version":999,"generator":"x","document":{}}"#;
        assert!(matches!(from_str(s), Err(IoError::NewerFormat(999))));
    }
}
