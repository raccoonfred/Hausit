//! Stage 4: the compressed data streams.
//!
//! `Formats/Latest`, the `Global/*` streams and the `Partitions/*` streams
//! hold gzip-compressed data. `Global` streams start with an 8-byte
//! header, `Formats/Latest` is one gzip member, and a partition is a short
//! header followed by many gzip members, one after another. This stage
//! finds every member and inflates it.

use flate2::{Decompress, FlushDecompress, Status};

/// A stream's data after inflating its gzip members, joined in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inflated {
    /// The gzip members found, as (offset in the stream, inflated size).
    pub members: Vec<(usize, usize)>,
    pub data: Vec<u8>,
}

/// The length of a gzip header at `d`, if it is one (RFC 1952).
fn gzip_header(d: &[u8]) -> Option<usize> {
    if d.len() < 10 || d[0] != 0x1f || d[1] != 0x8b || d[2] != 8 || d[3] & 0xe0 != 0 {
        return None;
    }
    let flags = d[3];
    let mut i = 10;
    if flags & 4 != 0 {
        let n = u16::from_le_bytes([*d.get(i)?, *d.get(i + 1)?]) as usize;
        i += 2 + n;
    }
    for bit in [8u8, 16] {
        if flags & bit != 0 {
            i += d.get(i..)?.iter().position(|b| *b == 0)? + 1;
        }
    }
    if flags & 2 != 0 {
        i += 2;
    }
    (i <= d.len()).then_some(i)
}

/// Inflates the raw deflate data at `d`; returns the data and how many
/// input bytes it used. The gzip trailer is not checked: some writers
/// leave it out.
fn inflate(d: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut z = Decompress::new(false);
    let mut out: Vec<u8> = Vec::with_capacity(d.len() * 4);
    loop {
        if out.capacity() - out.len() < 64 * 1024 {
            out.reserve(out.capacity().max(64 * 1024));
        }
        let (before_in, before_out) = (z.total_in(), z.total_out());
        let status = z.decompress_vec(&d[before_in as usize..], &mut out, FlushDecompress::None).ok()?;
        match status {
            Status::StreamEnd => return Some((out, z.total_in() as usize)),
            Status::Ok | Status::BufError => {
                if z.total_in() == before_in && z.total_out() == before_out {
                    // No progress: the data is truncated.
                    return (!out.is_empty()).then(|| (out, z.total_in() as usize));
                }
            }
        }
    }
}

/// Finds and inflates every gzip member in a stream.
pub fn inflate_stream(d: &[u8]) -> Inflated {
    let mut r = Inflated::default();
    let mut i = 0;
    while i + 10 <= d.len() {
        let Some(off) = d[i..].windows(3).position(|w| w == [0x1f, 0x8b, 8]).map(|p| p + i) else {
            break;
        };
        let parsed = gzip_header(&d[off..]).and_then(|h| inflate(&d[off + h..]).map(|(data, used)| (data, h + used)));
        match parsed {
            Some((data, used)) => {
                r.members.push((off, data.len()));
                r.data.extend_from_slice(&data);
                // Skip the 8-byte trailer when it is there.
                i = off + used;
            }
            None => i = off + 1,
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use std::io::Write;

    fn gz(data: &[u8]) -> Vec<u8> {
        let mut e = GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn inflates_members_after_a_header() {
        let mut stream = vec![9, 0, 0, 0, 0, 0, 0, 0, 0x31, 3];
        stream.extend(gz(b"first member "));
        stream.extend([1, 2, 3]);
        stream.extend(gz(&b"second member".repeat(100)));
        let r = inflate_stream(&stream);
        assert_eq!(r.members.len(), 2);
        assert!(r.data.starts_with(b"first member second member"));
        assert_eq!(r.data.len(), 13 + 1300);
    }

    #[test]
    fn tolerates_a_missing_trailer() {
        let mut g = gz(b"no trailer here");
        g.truncate(g.len() - 8);
        assert_eq!(inflate_stream(&g).data, b"no trailer here");
    }
}
