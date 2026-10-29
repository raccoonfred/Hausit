//! A reader for ISO 10303-21 ("STEP") files, the text format of IFC: the
//! schema named in the header and every `#id=ENTITY(...)` instance of the
//! DATA section, with its attributes parsed.

use std::collections::HashMap;

/// An attribute value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `$`: unset.
    Null,
    /// `*`: derived.
    Derived,
    Ref(u32),
    Int(i64),
    Real(f64),
    Str(String),
    /// `.ENUM.`, without the dots; `.T.` and `.F.` too.
    Enum(String),
    List(Vec<Value>),
    /// A typed value such as `IFCLABEL('x')`.
    Typed(String, Box<Value>),
}

impl Value {
    pub fn as_ref(&self) -> Option<u32> {
        match self {
            Value::Ref(r) => Some(*r),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Real(v) => Some(*v),
            Value::Int(v) => Some(*v as f64),
            Value::Typed(_, v) => v.as_f64(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            Value::Typed(_, v) => v.as_str(),
            _ => None,
        }
    }

    pub fn as_enum(&self) -> Option<&str> {
        match self {
            Value::Enum(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_list(&self) -> &[Value] {
        match self {
            Value::List(l) => l,
            _ => &[],
        }
    }

    /// References in a list attribute.
    pub fn refs(&self) -> Vec<u32> {
        self.as_list().iter().filter_map(|v| v.as_ref()).collect()
    }
}

/// One instance: its entity name (upper case) and attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub name: String,
    pub args: Vec<Value>,
}

impl Entity {
    pub fn arg(&self, i: usize) -> &Value {
        self.args.get(i).unwrap_or(&Value::Null)
    }
}

/// A parsed STEP file.
#[derive(Debug, Default)]
pub struct StepFile {
    /// The first schema in FILE_SCHEMA, such as `IFC4` or `IFC2X3`.
    pub schema: String,
    pub entities: HashMap<u32, Entity>,
}

impl StepFile {
    pub fn get(&self, id: u32) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// Ids of all instances of an entity, in file order.
    pub fn all(&self, name: &str) -> Vec<u32> {
        let mut ids: Vec<u32> = self.entities.iter().filter(|(_, e)| e.name == name).map(|(id, _)| *id).collect();
        ids.sort_unstable();
        ids
    }
}

/// Splits the file into records (`...;`), skipping comments and keeping
/// semicolons inside strings.
fn records(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut chars = text.chars().peekable();
    let mut in_str = false;
    while let Some(c) = chars.next() {
        if in_str {
            cur.push(c);
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    cur.push(chars.next().unwrap());
                } else {
                    in_str = false;
                }
            }
            continue;
        }
        match c {
            '\'' => {
                in_str = true;
                cur.push(c);
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = ' ';
                for d in chars.by_ref() {
                    if prev == '*' && d == '/' {
                        break;
                    }
                    prev = d;
                }
            }
            ';' => {
                out.push(std::mem::take(&mut cur).trim().to_string());
            }
            '\r' | '\n' => {}
            _ => cur.push(c),
        }
    }
    out
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }

    fn list(&mut self) -> Result<Vec<Value>, String> {
        // At '('.
        self.i += 1;
        let mut items = Vec::new();
        loop {
            match self.peek() {
                Some(b')') => {
                    self.i += 1;
                    return Ok(items);
                }
                Some(b',') => self.i += 1,
                Some(_) => items.push(self.value()?),
                None => return Err("unterminated list".into()),
            }
        }
    }

    fn value(&mut self) -> Result<Value, String> {
        let c = self.peek().ok_or("missing value")?;
        match c {
            b'$' => {
                self.i += 1;
                Ok(Value::Null)
            }
            b'*' => {
                self.i += 1;
                Ok(Value::Derived)
            }
            b'#' => {
                self.i += 1;
                let start = self.i;
                while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                    self.i += 1;
                }
                let n = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
                n.parse().map(Value::Ref).map_err(|_| format!("bad reference #{n}"))
            }
            b'(' => self.list().map(Value::List),
            b'\'' => {
                self.i += 1;
                let mut raw = Vec::new();
                loop {
                    let Some(&b) = self.s.get(self.i) else {
                        return Err("unterminated string".into());
                    };
                    self.i += 1;
                    if b == b'\'' {
                        if self.s.get(self.i) == Some(&b'\'') {
                            raw.push(b'\'');
                            self.i += 1;
                        } else {
                            break;
                        }
                    } else {
                        raw.push(b);
                    }
                }
                Ok(Value::Str(decode(&String::from_utf8_lossy(&raw))))
            }
            b'.' => {
                self.i += 1;
                let start = self.i;
                while self.i < self.s.len() && self.s[self.i] != b'.' {
                    self.i += 1;
                }
                let e = String::from_utf8_lossy(&self.s[start..self.i]).to_string();
                self.i += 1;
                Ok(Value::Enum(e))
            }
            b'"' => {
                // Binary: kept as text.
                self.i += 1;
                let start = self.i;
                while self.i < self.s.len() && self.s[self.i] != b'"' {
                    self.i += 1;
                }
                let v = String::from_utf8_lossy(&self.s[start..self.i]).to_string();
                self.i += 1;
                Ok(Value::Str(v))
            }
            b'-' | b'+' | b'0'..=b'9' => {
                let start = self.i;
                self.i += 1;
                while self.i < self.s.len() && matches!(self.s[self.i], b'0'..=b'9' | b'.' | b'E' | b'e' | b'-' | b'+') {
                    self.i += 1;
                }
                let t = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("0");
                if t.contains(['.', 'E', 'e']) {
                    t.parse().map(Value::Real).map_err(|_| format!("bad number {t}"))
                } else {
                    t.parse().map(Value::Int).map_err(|_| format!("bad number {t}"))
                }
            }
            c if c.is_ascii_alphabetic() => {
                let start = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_') {
                    self.i += 1;
                }
                let name = String::from_utf8_lossy(&self.s[start..self.i]).to_ascii_uppercase();
                if self.peek() != Some(b'(') {
                    return Err(format!("expected ( after {name}"));
                }
                let mut args = self.list()?;
                let inner = if args.len() == 1 { args.remove(0) } else { Value::List(args) };
                Ok(Value::Typed(name, Box::new(inner)))
            }
            other => Err(format!("unexpected character '{}'", other as char)),
        }
    }
}

/// Decodes STEP string escapes: `\X2\hhhh...\X0\` (UTF-16), `\X\hh`
/// (Latin-1) and `\\`.
fn decode(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if let Some(r) = rest.strip_prefix("\\X2\\") {
            let end = r.find("\\X0\\").unwrap_or(r.len());
            let hex = &r[..end];
            let units: Vec<u16> = (0..hex.len() / 4)
                .filter_map(|k| u16::from_str_radix(&hex[k * 4..k * 4 + 4], 16).ok())
                .collect();
            out.push_str(&String::from_utf16_lossy(&units));
            rest = r.get(end + 4..).unwrap_or("");
        } else if let Some(r) = rest.strip_prefix("\\X\\") {
            if let Some(b) = r.get(..2).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(b as char);
                rest = &r[2..];
            } else {
                rest = r;
            }
        } else if let Some(r) = rest.strip_prefix("\\\\") {
            out.push('\\');
            rest = r;
        } else if rest.starts_with("\\S\\") && rest.len() >= 4 {
            // \S\c: the character c + 128 (ISO 8859).
            let c = rest.as_bytes()[3];
            out.push(char::from(c.wrapping_add(128)));
            rest = &rest[4..];
        } else {
            out.push('\\');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// Parses a STEP file.
pub fn parse(text: &str) -> Result<StepFile, String> {
    let mut file = StepFile::default();
    let mut in_data = false;
    let mut errors = 0;
    for rec in records(text) {
        if rec.is_empty() {
            continue;
        }
        let upper = rec.to_ascii_uppercase();
        if upper.starts_with("FILE_SCHEMA") {
            if let Some(start) = rec.find('\'') {
                if let Some(len) = rec[start + 1..].find('\'') {
                    file.schema = rec[start + 1..start + 1 + len].to_ascii_uppercase();
                }
            }
            continue;
        }
        if upper == "DATA" || upper.starts_with("DATA(") || upper.starts_with("DATA (") {
            in_data = true;
            continue;
        }
        if upper == "ENDSEC" {
            in_data = false;
            continue;
        }
        if !in_data || !rec.starts_with('#') {
            continue;
        }
        let Some(eq) = rec.find('=') else { continue };
        let Ok(id) = rec[1..eq].trim().parse::<u32>() else { continue };
        let body = rec[eq + 1..].trim();
        let Some(open) = body.find('(') else { continue };
        let name = body[..open].trim().to_ascii_uppercase();
        let mut p = Parser {
            s: &body.as_bytes()[open..],
            i: 0,
        };
        match p.list() {
            Ok(args) => {
                file.entities.insert(id, Entity { name, args });
            }
            Err(_) => errors += 1,
        }
    }
    if file.entities.is_empty() {
        return Err(if errors > 0 {
            "the file's data could not be read".into()
        } else {
            "the file has no data section".into()
        });
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_records() {
        let text = "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC4'));\nENDSEC;\nDATA;\n\
            /* a comment; with a semicolon */\n\
            #1=IFCCARTESIANPOINT((0.,1.5,-2.E-3));\n\
            #2=IFCWALL('2O2Fr$t4X7Zf',$,'It''s; a \\X2\\00E9\\X0\\',$,$,#5,*,.T.,\n.SOLIDWALL.);\n\
            #3=IFCPROPERTYSINGLEVALUE('W',$,IFCLENGTHMEASURE(200.),$);\n\
            ENDSEC;\nEND-ISO-10303-21;\n";
        let f = parse(text).unwrap();
        assert_eq!(f.schema, "IFC4");
        let p = f.get(1).unwrap();
        assert_eq!(p.name, "IFCCARTESIANPOINT");
        let c: Vec<f64> = p.arg(0).as_list().iter().filter_map(|v| v.as_f64()).collect();
        assert_eq!(c, vec![0.0, 1.5, -0.002]);
        let w = f.get(2).unwrap();
        assert_eq!(w.arg(2).as_str(), Some("It's; a é"));
        assert_eq!(w.arg(5).as_ref(), Some(5));
        assert_eq!(w.arg(6), &Value::Derived);
        assert_eq!(w.arg(8).as_enum(), Some("SOLIDWALL"));
        assert_eq!(f.get(3).unwrap().arg(2).as_f64(), Some(200.0));
        assert_eq!(f.all("IFCWALL"), vec![2]);
    }
}
