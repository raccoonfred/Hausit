//! Stage 2: the `BasicFileInfo` stream.
//!
//! The stream starts with a small binary header followed by UTF-16LE text
//! made of `Key: value` lines. Newer files (2019+) contain `Format: 20xx`;
//! older ones contain `Revit Build: Autodesk Revit 20xx (Build: …)`.

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BasicFileInfo {
    /// Revit release year the file was saved in, e.g. 2024.
    pub version: Option<u32>,
    /// Product string for older files, e.g. "Autodesk Revit 2016".
    pub product: Option<String>,
    pub build: Option<String>,
    pub worksharing: Option<String>,
    pub username: Option<String>,
    pub last_save_path: Option<String>,
    pub locale: Option<String>,
}

impl BasicFileInfo {
    pub fn version_label(&self) -> String {
        match (&self.product, self.version) {
            (Some(p), _) => p.clone(),
            (None, Some(v)) => format!("Autodesk Revit {v}"),
            (None, None) => "unknown Revit version".into(),
        }
    }
}

/// Decodes UTF-16LE text starting at an even or odd offset, keeping
/// printable characters and line breaks.
fn decode_utf16(bytes: &[u8], offset: usize) -> String {
    let units: Vec<u16> = bytes[offset.min(bytes.len())..]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    char::decode_utf16(units)
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .map(|c| if c == '\r' || c == '\n' || !c.is_control() { c } else { '\n' })
        .collect()
}

fn year_after(text: &str, marker: &str) -> Option<(u32, usize)> {
    let i = text.find(marker)? + marker.len();
    let digits: String = text[i..].chars().take_while(|c| c.is_ascii_digit()).collect();
    let y: u32 = digits.parse().ok()?;
    (2000..2100).contains(&y).then_some((y, i))
}

pub fn parse(bytes: &[u8]) -> Option<BasicFileInfo> {
    // The binary header length varies by version; try both alignments and
    // keep whichever decodes into recognisable text.
    let text = [0usize, 1]
        .iter()
        .map(|o| decode_utf16(bytes, *o))
        .max_by_key(|t| ["Format:", "Build:", "Worksharing:"].iter().filter(|m| t.contains(*m)).count())?;
    let line_value = |key: &str| -> Option<String> {
        text.lines()
            .find_map(|l| {
                l.trim_start_matches('\u{FFFD}')
                    .trim()
                    .strip_prefix(key)
                    .map(|v| v.trim().to_string())
            })
            .filter(|v| !v.is_empty())
    };
    let mut info = BasicFileInfo {
        worksharing: line_value("Worksharing:"),
        username: line_value("Username:"),
        last_save_path: line_value("Last Save Path:"),
        locale: line_value("Locale when saved:"),
        ..Default::default()
    };
    if let Some((y, _)) = year_after(&text, "Format: ") {
        info.version = Some(y);
        info.build = line_value("Build:");
    }
    // Legacy: "Revit Build: Autodesk Revit Architecture 2012 (Build: …)"
    if let Some(i) = text.find("Autodesk Revit") {
        let rest = &text[i..];
        let product: String = rest.chars().take_while(|c| *c != '(' && *c != '\r' && *c != '\n').collect();
        let product = product.trim().to_string();
        if let Some(y) = product
            .split_whitespace()
            .filter_map(|w| w.parse::<u32>().ok())
            .find(|y| (2000..2100).contains(y))
        {
            info.version.get_or_insert(y);
        }
        if let Some(b) = rest.find("(Build: ") {
            // The build itself contains "(x64)", so read up to the "))".
            let after = &rest[b + 8..];
            let end = after.find("))").map(|e| e + 1).or_else(|| after.find(')')).unwrap_or(after.len());
            info.build.get_or_insert_with(|| after[..end].to_string());
        }
        info.product = Some(product);
    }
    (info.version.is_some() || info.worksharing.is_some()).then_some(info)
}
