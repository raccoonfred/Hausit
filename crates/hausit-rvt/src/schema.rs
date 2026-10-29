//! Stage 5: the class schema in `Formats/Latest`.
//!
//! Once inflated, the stream lists the classes the file's elements are
//! made of, each with the names of its fields: a class name is a 16-bit
//! length and the name, followed by a 16-bit tag; when the tag's high bit
//! is set, a 16-bit zero and the parent class's record follow. Field
//! names are a 32-bit length and the name; they are listed under the
//! class record they follow. This stage reads the names and the class
//! hierarchy; the encoding of each field's type is not decoded yet.

/// One class of the schema.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassDef {
    pub name: String,
    /// The tag after the name; with the high bit set it is the class's
    /// number and a parent follows.
    pub tag: u16,
    pub parent: Option<String>,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Schema {
    pub classes: Vec<ClassDef>,
}

impl Schema {
    pub fn class(&self, name: &str) -> Option<&ClassDef> {
        self.classes
            .iter()
            .find(|c| c.name == name && !c.fields.is_empty())
            .or_else(|| self.classes.iter().find(|c| c.name == name))
    }

    /// The class and its ancestors, nearest first.
    pub fn ancestry(&self, name: &str) -> Vec<String> {
        let mut out = vec![name.to_string()];
        let mut cur = name.to_string();
        while let Some(p) = self.classes.iter().find(|c| c.name == cur).and_then(|c| c.parent.clone()) {
            if out.contains(&p) || out.len() > 32 {
                break;
            }
            out.push(p.clone());
            cur = p;
        }
        out
    }

    /// Classes with distinct names.
    pub fn class_names(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.classes.iter().map(|c| c.name.as_str()).collect();
        v.sort_unstable();
        v.dedup();
        v
    }
}

fn is_name(s: &[u8]) -> bool {
    let ok_first = |c: u8| c.is_ascii_alphabetic() || c == b'_';
    let ok = |c: u8| c.is_ascii_alphanumeric() || b"_:<>, *&".contains(&c);
    !s.is_empty() && ok_first(s[0]) && s.iter().all(|c| ok(*c)) && s.last() != Some(&b' ')
}

/// A name with an `n`-byte little-endian length prefix at `i`.
fn name_at(d: &[u8], i: usize, n: usize) -> Option<&str> {
    let len_bytes = d.get(i..i + n)?;
    let len = len_bytes.iter().rev().fold(0usize, |a, b| (a << 8) | *b as usize);
    if !(2..=200).contains(&len) {
        return None;
    }
    let s = d.get(i + n..i + n + len)?;
    is_name(s).then(|| std::str::from_utf8(s).ok()).flatten()
}

/// Reads the class and field names from inflated `Formats/Latest` data.
pub fn parse(d: &[u8]) -> Schema {
    let mut classes: Vec<ClassDef> = Vec::new();
    let mut i = 0;
    while i + 4 < d.len() {
        // Field names: 32-bit length.
        if let Some(f) = name_at(d, i, 4) {
            if let Some(c) = classes.last_mut() {
                c.fields.push(f.to_string());
            }
            i += 4 + f.len();
            continue;
        }
        // Class names: 16-bit length, then the tag.
        if let Some(name) = name_at(d, i, 2).filter(|n| !n.starts_with("m_")) {
            let j = i + 2 + name.len();
            let tag = d.get(j..j + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).unwrap_or(0);
            let mut next = j + 2;
            let mut parent = None;
            if tag & 0x8000 != 0 && d.get(next..next + 2) == Some(&[0, 0]) {
                if let Some(p) = name_at(d, next + 2, 2) {
                    parent = Some(p.to_string());
                    // The parent's own record follows: read it next.
                    next += 2;
                }
            }
            classes.push(ClassDef {
                name: name.to_string(),
                tag,
                parent,
                fields: Vec::new(),
            });
            i = next;
            continue;
        }
        i += 1;
    }
    Schema { classes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(name: &str, tag: u16, parent: Option<&str>, fields: &[&str]) -> Vec<u8> {
        let mut v = (name.len() as u16).to_le_bytes().to_vec();
        v.extend(name.as_bytes());
        v.extend(tag.to_le_bytes());
        if let Some(p) = parent {
            v.extend([0, 0]);
            v.extend((p.len() as u16).to_le_bytes());
            v.extend(p.as_bytes());
        }
        v.extend([0, 0, 1, 0, 0, 0]);
        v.extend((fields.len() as u32).to_le_bytes());
        for f in fields {
            v.extend((f.len() as u32).to_le_bytes());
            v.extend(f.as_bytes());
            v.extend([0x0e, 0, 0, 0, 0, 0, 0, 0]);
        }
        v
    }

    /// A synthetic stream in the layout described above.
    #[test]
    fn reads_classes_fields_and_parents() {
        let mut d = vec![0, 0];
        d.extend(class("Element", 0, None, &["m_docAccess"]));
        // A class whose parent's record follows inline.
        let mut wall = (4u16).to_le_bytes().to_vec();
        wall.extend(b"Wall");
        wall.extend(0x8005u16.to_le_bytes());
        wall.extend([0, 0]);
        wall.extend(class("HostObj", 0x8006, Some("Element"), &["m_location", "m_height"]));
        d.extend(wall);
        let s = parse(&d);
        assert_eq!(s.class("Wall").unwrap().parent.as_deref(), Some("HostObj"));
        assert_eq!(s.ancestry("Wall"), vec!["Wall", "HostObj", "Element"]);
        assert_eq!(s.class("Element").unwrap().fields, vec!["m_docAccess"]);
        assert!(s.class_names().contains(&"HostObj"));
    }
}
