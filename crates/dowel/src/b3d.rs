//! BZ85 container: Bazis-Mebelshchik models (`.b3d`) and hardware (`.fr3d`).

use std::fmt;
use std::io::Read;

const MAGIC: &[u8; 4] = b"BZ85";
const SECTION: [u8; 4] = [0x01, 0x00, 0x00, 0xff];
/// Protection against hostile files: real models have tree depth < 30.
const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Type 0: container node, contents in `children`.
    Obj,
    True,
    False,
    U8(u8),
    I32(i32),
    F64(f64),
    Str(String),
    Blob(Vec<u8>),
    /// Type 8: empty value.
    Nil,
    /// Type 9: Delphi TDateTime (days since 1899-12-30).
    DateTime(f64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Empty name for list elements.
    pub name: String,
    pub value: Value,
    pub children: Vec<Node>,
}

impl Node {
    pub fn get(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn f64(&self, name: &str) -> Option<f64> {
        match self.get(name)?.value {
            Value::F64(v) => Some(v),
            Value::I32(v) => Some(v.into()),
            Value::U8(v) => Some(v.into()),
            _ => None,
        }
    }
    /// Integers are stored either as u8 or as i32, depending on magnitude.
    pub fn int(&self, name: &str) -> Option<i64> {
        match self.get(name)?.value {
            Value::U8(v) => Some(v.into()),
            Value::I32(v) => Some(v.into()),
            _ => None,
        }
    }
    pub fn str(&self, name: &str) -> Option<&str> {
        match &self.get(name)?.value {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }
    pub fn blob(&self, name: &str) -> Option<&[u8]> {
        match &self.get(name)?.value {
            Value::Blob(b) => Some(b),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Error {
    pub offset: usize,
    pub msg: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "b3d: {} (offset {})", self.msg, self.offset)
    }
}
impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// File: an uncompressed header section (`Header`: version, preview, article number),
/// then a compressed document section (`Document`: the model). The tail after the sections
/// (64 bytes in `.b3d`, probably a signature) is not parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct File {
    pub sections: Vec<Node>,
}

impl File {
    pub fn section(&self, name: &str) -> Option<&Node> {
        self.sections.iter().find(|s| s.name == name)
    }
}

pub fn parse(data: &[u8]) -> Result<File> {
    if data.get(..4) != Some(MAGIC) {
        return Err(Error { offset: 0, msg: "no BZ85 signature".into() });
    }
    let mut r = Reader { b: data, p: 4 };
    let mut sections = Vec::new();
    while r.b.get(r.p..r.p + 4) == Some(&SECTION) {
        r.p += 4;
        if r.u8()? != 0 {
            let at = r.p;
            let mut z = flate2::read::ZlibDecoder::new(&r.b[r.p..]);
            let mut body = Vec::new();
            z.read_to_end(&mut body)
                .map_err(|e| Error { offset: at, msg: format!("zlib: {e}") })?;
            r.p += z.total_in() as usize;
            let mut sub = Reader { b: &body, p: 0 };
            sections.push(sub.section()?);
        } else {
            sections.push(r.section()?);
        }
    }
    if sections.is_empty() {
        return Err(Error { offset: r.p, msg: "no sections".into() });
    }
    Ok(File { sections })
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Reader<'a> {
    fn err<T>(&self, msg: impl Into<String>) -> Result<T> {
        Err(Error { offset: self.p, msg: msg.into() })
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        match self.b.get(self.p..self.p.saturating_add(n)) {
            Some(s) => {
                self.p += n;
                Ok(s)
            }
            None => self.err(format!("data ends early, {n} bytes needed")),
        }
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn len(&mut self, unit: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if n.saturating_mul(unit) > self.b.len() - self.p {
            return self.err(format!("length {n} exceeds the rest of the file"));
        }
        Ok(n)
    }

    /// Name dictionary (u32 n, then n × [u32 len, cp1251/ASCII]) and the root node.
    fn section(&mut self) -> Result<Node> {
        let n = self.len(4)?;
        let names = (0..n)
            .map(|_| {
                let k = self.len(1)?;
                Ok(self.take(k)?.iter().map(|&c| c as char).collect())
            })
            .collect::<Result<Vec<String>>>()?;
        self.node(&names, 0)
    }

    /// Node: i32 name index (-1 for list elements), u32 child count, u8 type, value, children.
    fn node(&mut self, names: &[String], depth: usize) -> Result<Node> {
        if depth > MAX_DEPTH {
            return self.err("tree too deep");
        }
        let at = self.p;
        let ni = self.i32()?;
        let name = match ni {
            -1 => String::new(),
            _ => match names.get(ni as usize) {
                Some(s) => s.clone(),
                None => return Err(Error { offset: at, msg: format!("name index {ni} outside the dictionary") }),
            },
        };
        // Every node takes at least 9 bytes — this rejects fake huge counts.
        let n = self.len(9)?;
        let value = match self.u8()? {
            0 => Value::Obj,
            1 => Value::True,
            2 => Value::False,
            3 => Value::U8(self.u8()?),
            4 => Value::I32(self.i32()?),
            5 => Value::F64(self.f64()?),
            6 => {
                let k = self.len(2)?;
                let units: Vec<u16> =
                    self.take(2 * k)?.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
                // Some Bazis strings contain lone surrogates.
                Value::Str(String::from_utf16_lossy(&units))
            }
            7 => {
                let k = self.len(1)?;
                Value::Blob(self.take(k)?.to_vec())
            }
            8 => Value::Nil,
            9 => Value::DateTime(self.f64()?),
            t => return Err(Error { offset: at, msg: format!("unknown type {t} of “{name}”") }),
        };
        let children = (0..n).map(|_| self.node(names, depth + 1)).collect::<Result<_>>()?;
        Ok(Node { name, value, children })
    }
}

/// Writes a BZ85 container: the `Header` section uncompressed, the rest zlib (as Bazis does).
/// The name dictionary is rebuilt; numbers keep their form (u8/i32/f64).
/// shortcut: the 64-byte tail (probably a Bazis signature) is not written — its structure is unknown.
pub fn write(file: &File) -> Vec<u8> {
    use std::io::Write;
    let mut out = MAGIC.to_vec();
    for s in &file.sections {
        out.extend_from_slice(&SECTION);
        let body = section_bytes(s);
        if s.name == "Header" {
            out.push(0);
            out.extend_from_slice(&body);
        } else {
            out.push(1);
            let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            z.write_all(&body).expect("writing to memory");
            out.extend_from_slice(&z.finish().expect("writing to memory"));
        }
    }
    out
}

fn section_bytes(root: &Node) -> Vec<u8> {
    fn collect<'a>(n: &'a Node, names: &mut Vec<&'a str>) {
        if !n.name.is_empty() && !names.contains(&n.name.as_str()) {
            names.push(&n.name);
        }
        n.children.iter().for_each(|c| collect(c, names));
    }
    let mut names = vec![""];
    collect(root, &mut names);
    let mut out = Vec::new();
    out.extend_from_slice(&(names.len() as u32).to_le_bytes());
    for n in &names {
        // Names are Latin: one char = one byte (that is how `section` reads them).
        let bytes: Vec<u8> = n.chars().map(|c| c as u32 as u8).collect();
        out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&bytes);
    }
    let index = |name: &str| -> i32 { if name.is_empty() { -1 } else { names.iter().position(|n| *n == name).expect("name in the dictionary") as i32 } };
    fn node(n: &Node, index: &dyn Fn(&str) -> i32, out: &mut Vec<u8>, root: bool) {
        // The section root is written with its own name index; list elements with −1.
        let i = if root && n.name.is_empty() { 0 } else { index(&n.name) };
        out.extend_from_slice(&i.to_le_bytes());
        out.extend_from_slice(&(n.children.len() as u32).to_le_bytes());
        match &n.value {
            Value::Obj => out.push(0),
            Value::True => out.push(1),
            Value::False => out.push(2),
            Value::U8(v) => out.extend_from_slice(&[3, *v]),
            Value::I32(v) => {
                out.push(4);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Value::F64(v) => {
                out.push(5);
                out.extend_from_slice(&v.to_le_bytes());
            }
            Value::Str(s) => {
                out.push(6);
                let units: Vec<u16> = s.encode_utf16().collect();
                out.extend_from_slice(&(units.len() as u32).to_le_bytes());
                units.iter().for_each(|u| out.extend_from_slice(&u.to_le_bytes()));
            }
            Value::Blob(b) => {
                out.push(7);
                out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                out.extend_from_slice(b);
            }
            Value::Nil => out.push(8),
            Value::DateTime(v) => {
                out.push(9);
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        n.children.iter().for_each(|c| node(c, index, out, false));
    }
    node(root, &index, &mut out, true);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_parse_is_identity() {
        let leaf = |name: &str, value: Value| Node { name: name.into(), value, children: Vec::new() };
        let file = File {
            sections: vec![
                Node { name: "Header".into(), value: Value::Obj, children: vec![leaf("Version", Value::U8(15)), leaf("Thumbnail", Value::Blob(vec![1, 2, 3]))] },
                Node {
                    name: "Document".into(),
                    value: Value::Obj,
                    children: vec![Node {
                        name: "Model".into(),
                        value: Value::Obj,
                        // Cyrillic on purpose: the name must round-trip through the Bazis code page.
                        children: vec![Node { name: String::new(), value: Value::Obj, children: vec![leaf("Name", Value::Str("\u{41f}\u{43e}\u{43b}\u{43a}\u{430} \u{ab}1\u{bb}".into())), leaf("ID", Value::I32(-7)), leaf("X", Value::F64(1.5)), leaf("Clip", Value::False), leaf("N", Value::Nil), leaf("T", Value::DateTime(46000.5))] }],
                    }],
                },
            ],
        };
        assert_eq!(parse(&write(&file)).unwrap(), file);
    }
}
