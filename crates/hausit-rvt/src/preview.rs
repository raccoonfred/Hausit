//! Stage 3: the thumbnail in `RevitPreview4.0`, a PNG after a short header.

const PNG_SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
const IEND: [u8; 8] = [b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82];

pub fn extract_png(stream: &[u8]) -> Option<Vec<u8>> {
    let start = stream.windows(8).position(|w| w == PNG_SIG)?;
    let end = stream[start..].windows(8).position(|w| w == IEND)? + start + 8;
    Some(stream[start..end].to_vec())
}
