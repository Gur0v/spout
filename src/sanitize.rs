use crate::error::{Result, SpoutError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanitizeStatus {
    Cleaned(&'static str),
    Kept(&'static str),
    Unknown,
    Disabled,
}

#[derive(Debug)]
pub struct Sanitized {
    pub bytes: Vec<u8>,
    pub status: SanitizeStatus,
}

pub fn sanitize_media(bytes: Vec<u8>, enabled: bool) -> Result<Sanitized> {
    if !enabled {
        return Ok(Sanitized {
            bytes,
            status: SanitizeStatus::Disabled,
        });
    }

    if is_png(&bytes) {
        return sanitize_known(bytes, "png", sanitize_png);
    }

    if is_jpeg(&bytes) {
        return sanitize_known(bytes, "jpeg", sanitize_jpeg);
    }

    if is_webp(&bytes) {
        return sanitize_known(bytes, "webp", sanitize_webp);
    }

    Ok(Sanitized {
        bytes,
        status: SanitizeStatus::Unknown,
    })
}

fn sanitize_known(
    bytes: Vec<u8>,
    format: &'static str,
    sanitizer: fn(&[u8]) -> Option<Vec<u8>>,
) -> Result<Sanitized> {
    let sanitized = sanitizer(&bytes).ok_or(SpoutError::SanitizeFailed(format))?;
    let status = if sanitized == bytes {
        SanitizeStatus::Kept(format)
    } else {
        SanitizeStatus::Cleaned(format)
    };

    Ok(Sanitized {
        bytes: sanitized,
        status,
    })
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes.starts_with(&[0xFF, 0xD8])
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

fn sanitize_png(bytes: &[u8]) -> Option<Vec<u8>> {
    const SIG: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(SIG);

    let mut pos = SIG.len();
    while pos + 12 <= bytes.len() {
        let length = u32::from_be_bytes(bytes[pos..pos + 4].try_into().ok()?) as usize;
        let end = pos.checked_add(12 + length)?;
        if end > bytes.len() {
            return None;
        }

        let kind = &bytes[pos + 4..pos + 8];
        if !matches!(kind, b"eXIf" | b"tEXt" | b"zTXt" | b"iTXt" | b"iCCP" | b"tIME") {
            out.extend_from_slice(&bytes[pos..end]);
        }

        pos = end;

        if kind == b"IEND" {
            out.extend_from_slice(&bytes[pos..]);
            return Some(out);
        }
    }

    None
}

fn sanitize_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[..2]);

    let mut pos = 2;
    while pos < bytes.len() {
        let marker_start = pos;
        if bytes[pos] != 0xFF {
            return None;
        }

        pos += 1;
        while pos < bytes.len() && bytes[pos] == 0xFF {
            pos += 1;
        }
        if pos >= bytes.len() {
            return None;
        }

        let marker = bytes[pos];
        pos += 1;

        if marker == 0xD9 {
            out.extend_from_slice(&bytes[marker_start..pos]);
            out.extend_from_slice(&bytes[pos..]);
            return Some(out);
        }

        if is_standalone_jpeg_marker(marker) {
            out.extend_from_slice(&bytes[marker_start..pos]);
            continue;
        }

        if pos + 2 > bytes.len() {
            return None;
        }

        let segment_len = u16::from_be_bytes(bytes[pos..pos + 2].try_into().ok()?) as usize;
        if segment_len < 2 {
            return None;
        }

        let end = pos.checked_add(segment_len)?;
        if end > bytes.len() {
            return None;
        }

        if marker == 0xDA {
            out.extend_from_slice(&bytes[marker_start..end]);
            out.extend_from_slice(&bytes[end..]);
            return Some(out);
        }

        if !should_strip_jpeg_segment(marker, &bytes[pos + 2..end]) {
            out.extend_from_slice(&bytes[marker_start..end]);
        }

        pos = end;
    }

    None
}

fn should_strip_jpeg_segment(marker: u8, payload: &[u8]) -> bool {
    marker == 0xE1
        || marker == 0xED
        || marker == 0xFE
        || (marker == 0xE2 && payload.starts_with(b"ICC_PROFILE\0"))
}

fn is_standalone_jpeg_marker(marker: u8) -> bool {
    matches!(marker, 0x01 | 0xD0..=0xD7)
}

fn sanitize_webp(bytes: &[u8]) -> Option<Vec<u8>> {
    let riff_size = u32::from_le_bytes(bytes[4..8].try_into().ok()?) as usize;
    if riff_size.checked_add(8)? != bytes.len() {
        return None;
    }

    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(b"WEBP");

    let mut pos = 12;
    let mut vp8x_flags_offset = None;
    while pos + 8 <= bytes.len() {
        let chunk = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().ok()?) as usize;
        let padded = size + (size & 1);
        let end = pos.checked_add(8)?.checked_add(padded)?;
        if end > bytes.len() {
            return None;
        }

        if !matches!(chunk, b"ICCP" | b"EXIF" | b"XMP ") {
            let out_pos = out.len();
            out.extend_from_slice(&bytes[pos..end]);
            if chunk == b"VP8X" && size >= 10 {
                vp8x_flags_offset = Some(out_pos + 8);
            }
        }

        pos = end;
    }

    if pos != bytes.len() {
        return None;
    }

    if let Some(offset) = vp8x_flags_offset {
        out[offset] &= !(0x20 | 0x08 | 0x04);
    }

    let size = (out.len() - 8) as u32;
    out[4..8].copy_from_slice(&size.to_le_bytes());
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{SanitizeStatus, sanitize_media};

    #[test]
    fn strips_png_metadata_and_icc() {
        let png = png_bytes(&[
            (b"IHDR", b"head"),
            (b"tEXt", b"text"),
            (b"iCCP", b"icc"),
            (b"tIME", b"time"),
            (b"IDAT", b"data"),
            (b"IEND", b""),
        ]);

        let sanitized = sanitize_media(png, true).unwrap();

        assert_eq!(sanitized.status, SanitizeStatus::Cleaned("png"));
        assert!(!contains(&sanitized.bytes, b"tEXt"));
        assert!(!contains(&sanitized.bytes, b"iCCP"));
        assert!(!contains(&sanitized.bytes, b"tIME"));
        assert!(contains(&sanitized.bytes, b"IHDR"));
        assert!(contains(&sanitized.bytes, b"IDAT"));
    }

    #[test]
    fn keeps_png_without_metadata() {
        let png = png_bytes(&[(b"IHDR", b"head"), (b"IDAT", b"data"), (b"IEND", b"")]);

        let sanitized = sanitize_media(png.clone(), true).unwrap();

        assert_eq!(sanitized.status, SanitizeStatus::Kept("png"));
        assert_eq!(sanitized.bytes, png);
    }

    #[test]
    fn strips_jpeg_metadata_and_icc() {
        let jpeg = jpeg_bytes(&[
            (0xE1, b"Exif\0\0meta"),
            (0xE2, b"ICC_PROFILE\0\x01\x01icc"),
            (0xED, b"iptc"),
            (0xFE, b"comment"),
            (0xE2, b"NOT_ICC"),
        ]);

        let sanitized = sanitize_media(jpeg, true).unwrap();

        assert_eq!(sanitized.status, SanitizeStatus::Cleaned("jpeg"));
        assert!(!contains(&sanitized.bytes, b"Exif\0\0"));
        assert!(!contains(&sanitized.bytes, b"ICC_PROFILE\0"));
        assert!(!contains(&sanitized.bytes, b"comment"));
        assert!(contains(&sanitized.bytes, b"NOT_ICC"));
    }

    #[test]
    fn strips_webp_metadata_and_icc() {
        let webp = webp_bytes(&[
            (b"VP8X", &[0x2C, 0, 0, 0, 1, 0, 0, 1, 0, 0]),
            (b"ICCP", b"icc"),
            (b"EXIF", b"meta"),
            (b"XMP ", b"xmp"),
            (b"VP8 ", b"data"),
        ]);

        let sanitized = sanitize_media(webp, true).unwrap();

        assert_eq!(sanitized.status, SanitizeStatus::Cleaned("webp"));
        assert!(!contains(&sanitized.bytes, b"ICCP"));
        assert!(!contains(&sanitized.bytes, b"EXIF"));
        assert!(!contains(&sanitized.bytes, b"XMP "));
        let vp8x = find_chunk(&sanitized.bytes, b"VP8X").unwrap();
        assert_eq!(sanitized.bytes[vp8x + 8] & 0x2C, 0);
    }

    #[test]
    fn malformed_known_format_fails() {
        let png = b"\x89PNG\r\n\x1a\nbroken".to_vec();
        let err = sanitize_media(png, true).unwrap_err();
        assert_eq!(err.to_string(), "failed to strip metadata from png");
    }

    #[test]
    fn strip_meta_false_skips_cleaning() {
        let png = png_bytes(&[(b"IHDR", b"head"), (b"tEXt", b"text"), (b"IEND", b"")]);
        let sanitized = sanitize_media(png.clone(), false).unwrap();
        assert_eq!(sanitized.status, SanitizeStatus::Disabled);
        assert_eq!(sanitized.bytes, png);
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    fn find_chunk(bytes: &[u8], chunk: &[u8; 4]) -> Option<usize> {
        bytes.windows(4).position(|w| w == chunk)
    }

    fn png_bytes(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        for (kind, data) in chunks {
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            out.extend_from_slice(*kind);
            out.extend_from_slice(data);
            out.extend_from_slice(&0u32.to_be_bytes());
        }
        out
    }

    fn jpeg_bytes(segments: &[(u8, &[u8])]) -> Vec<u8> {
        let mut out = vec![0xFF, 0xD8];
        for (marker, payload) in segments {
            out.push(0xFF);
            out.push(*marker);
            out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
            out.extend_from_slice(payload);
        }
        out.extend_from_slice(&[0xFF, 0xDA, 0x00, 0x02, 0x11, 0x22, 0x33, 0xFF, 0xD9]);
        out
    }

    fn webp_bytes(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut body = b"WEBP".to_vec();
        for (kind, data) in chunks {
            body.extend_from_slice(*kind);
            body.extend_from_slice(&(data.len() as u32).to_le_bytes());
            body.extend_from_slice(data);
            if data.len() % 2 == 1 {
                body.push(0);
            }
        }

        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }
}
