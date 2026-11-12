//! Reading a picture file that may be gzip-compressed: the documentation stores its plots as `.svgz` (an SVG drawing
//! compressed with gzip), and a documentation page opened as a notebook names them. Exports take them as drawings.
use std::io;
use std::path::Path;

/// `data` decompressed when it is a gzip stream (RFC 1952); `None` when it is not one or is damaged.
fn gunzip(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 18 || data[..3] != [0x1f, 0x8b, 8] {
        return None;
    }
    let flags = data[3];
    let mut at = 10;
    if flags & 4 != 0 {
        at += 2 + usize::from(u16::from_le_bytes([*data.get(at)?, *data.get(at + 1)?]));
    }
    for bit in [8, 16] {
        if flags & bit != 0 {
            at += data.get(at..)?.iter().position(|&b| b == 0)? + 1;
        }
    }
    if flags & 2 != 0 {
        at += 2;
    }
    miniz_oxide::inflate::decompress_to_vec(data.get(at..data.len() - 8)?).ok()
}

/// The bytes of the picture file `path`; a gzip-compressed one (`.svgz`) decompressed.
pub fn read_picture(path: &Path) -> io::Result<Vec<u8>> {
    let data = std::fs::read(path)?;
    if data.starts_with(&[0x1f, 0x8b]) {
        return gunzip(&data).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "a damaged compressed drawing"));
    }
    Ok(data)
}

/// The size in points of the picture file `path` whose bytes are `bytes`: as [`mc_nbfile::images::picture_size`], and
/// of a compressed drawing from its decompressed text.
pub fn picture_size(path: &Path, bytes: &[u8]) -> Option<(u32, u32)> {
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("svgz")) {
        return mc_nbfile::images::svg_size(&String::from_utf8_lossy(bytes));
    }
    mc_nbfile::images::picture_size(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_block_gzip_is_read() {
        // gzip header, one stored deflate block "hello", crc32 and length (not checked)
        let mut z = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 255, 1, 5, 0, 0xfa, 0xff];
        z.extend_from_slice(b"hello");
        z.extend_from_slice(&[0; 8]);
        assert_eq!(gunzip(&z).as_deref(), Some(&b"hello"[..]));
        assert_eq!(gunzip(b"plain text that is not gzip"), None);
    }
}
