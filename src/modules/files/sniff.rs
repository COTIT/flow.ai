//! An upload's type from its bytes. The client's file name and content type are never trusted:
//! magic numbers pick the type, ZIP-based office files are told apart by the names in their
//! central directory (read in place, bounded, nothing decompressed), old Office files by their
//! stream names, and text must be UTF-8 without NUL bytes. Markup (HTML, XHTML, XML, SVG),
//! scripts, programs and anything unrecognised get None.

/// Extension and content type of a recognised file.
pub type Kind = (&'static str, &'static str);

/// 422 for bytes of any other type.
pub const TYPES: &str = "The upload must be a file of type: png, jpeg, gif, webp, pdf, csv, txt, docx, docm, xlsx, xlsm, pptx, pptm, doc, xls, ppt, zip.";

const OLE: &[u8] = b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1";

/// The type of `b`; `name` only picks between the two text labels (`.csv` or not).
pub fn sniff(b: &[u8], name: &str) -> Option<Kind> {
    match b {
        [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', ..] => Some(("png", "image/png")),
        [0xff, 0xd8, 0xff, ..] => Some(("jpg", "image/jpeg")),
        [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some(("gif", "image/gif")),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some(("webp", "image/webp")),
        [b'%', b'P', b'D', b'F', b'-', ..] => Some(("pdf", "application/pdf")),
        [b'P', b'K', 3, 4, ..] | [b'P', b'K', 5, 6, ..] => zip(b),
        _ if b.starts_with(OLE) => ole(b),
        // Programs: PE, ELF, Mach-O (both byte orders, fat).
        [b'M', b'Z', ..] | [0x7f, b'E', b'L', b'F', ..] => None,
        [0xfe, 0xed, 0xfa, 0xce | 0xcf, ..] | [0xce | 0xcf, 0xfa, 0xed, 0xfe, ..] => None,
        [0xca, 0xfe, 0xba, 0xbe, ..] => None,
        _ => text(b, name),
    }
}

/// A ZIP by its central directory: OOXML (`[Content_Types].xml` plus `word/`, `xl/` or
/// `ppt/`; macro-enabled with a `vbaProject.bin`), else a plain archive. JARs and APKs are
/// programs; a directory that doesn't parse is unrecognised.
fn zip(b: &[u8]) -> Option<Kind> {
    let u16_at = |i: usize| Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?));
    let u32_at = |i: usize| Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?));
    // The end record sits in the last 22 bytes plus at most a 64 KiB comment.
    let last = b.len().checked_sub(22)?;
    let end = (last.saturating_sub(0xffff)..=last)
        .rev()
        .find(|&i| b[i..].starts_with(b"PK\x05\x06"))?;
    let entries = u16_at(end + 10)?;
    let size = usize::try_from(u32_at(end + 12)?).ok()?;
    let start = usize::try_from(u32_at(end + 16)?).ok()?;
    let stop = start.checked_add(size).filter(|&s| s <= end)?;
    let (mut types, mut word, mut xl, mut ppt, mut macros) = (false, false, false, false, false);
    let mut at = start;
    for _ in 0..entries {
        if b.get(at..at + 4)? != b"PK\x01\x02" {
            return None;
        }
        let name_len = usize::from(u16_at(at + 28)?);
        let skip = usize::from(u16_at(at + 30)?) + usize::from(u16_at(at + 32)?);
        let name = b.get(at + 46..at + 46 + name_len)?;
        let is = |n: &[u8]| name.eq_ignore_ascii_case(n);
        if is(b"META-INF/MANIFEST.MF") || is(b"AndroidManifest.xml") || is(b"classes.dex") {
            return None;
        }
        types |= name == b"[Content_Types].xml";
        word |= name.starts_with(b"word/");
        xl |= name.starts_with(b"xl/");
        ppt |= name.starts_with(b"ppt/");
        macros |= name.ends_with(b"/vbaProject.bin");
        at += 46 + name_len + skip;
        if at > stop {
            return None;
        }
    }
    Some(match (types, word, xl, ppt, macros) {
        (true, true, _, _, false) => (
            "docx",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        (true, true, _, _, true) => ("docm", "application/vnd.ms-word.document.macroEnabled.12"),
        (true, _, true, _, false) => (
            "xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
        (true, _, true, _, true) => ("xlsm", "application/vnd.ms-excel.sheet.macroEnabled.12"),
        (true, _, _, true, false) => (
            "pptx",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ),
        (true, _, _, true, true) => (
            "pptm",
            "application/vnd.ms-powerpoint.presentation.macroEnabled.12",
        ),
        _ => ("zip", "application/zip"),
    })
}

/// An OLE compound file by the stream names Word, Excel and PowerPoint write (UTF-16 in its
/// directory); any other (MSI installers included) is unrecognised.
fn ole(b: &[u8]) -> Option<Kind> {
    let has = |name: &str| {
        let utf16: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        b.windows(utf16.len()).any(|w| w == utf16)
    };
    if has("WordDocument") {
        Some(("doc", "application/msword"))
    } else if has("Workbook") || has("Book") {
        Some(("xls", "application/vnd.ms-excel"))
    } else if has("PowerPoint Document") {
        Some(("ppt", "application/vnd.ms-powerpoint"))
    } else {
        None
    }
}

/// UTF-8 text without NUL bytes that isn't markup (starts with `<`), a script (`#!`) or holds
/// a `<script` tag; CSV when named so, else plain text.
fn text(b: &[u8], name: &str) -> Option<Kind> {
    let s = std::str::from_utf8(b).ok()?;
    let head = s.trim_start_matches('\u{feff}').trim_start();
    let script = b.windows(7).any(|w| w.eq_ignore_ascii_case(b"<script"));
    if s.is_empty() || s.contains('\0') || head.starts_with('<') || head.starts_with("#!") || script
    {
        return None;
    }
    Some(if name.to_ascii_lowercase().ends_with(".csv") {
        ("csv", "text/csv; charset=utf-8")
    } else {
        ("txt", "text/plain; charset=utf-8")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_numbers_win() {
        assert_eq!(
            sniff(b"%PDF-1.7\n", "x.exe"),
            Some(("pdf", "application/pdf"))
        );
        assert_eq!(sniff(b"\xff\xd8\xff\xe0", "x"), Some(("jpg", "image/jpeg")));
        assert_eq!(sniff(b"GIF89a..", "x"), Some(("gif", "image/gif")));
        assert_eq!(
            sniff(b"RIFF\0\0\0\0WEBPVP8 ", "x"),
            Some(("webp", "image/webp"))
        );
        assert_eq!(
            sniff(b"\x89PNG\r\n\x1a\n<html>", "x.html"),
            Some(("png", "image/png"))
        );
        for program in [
            &b"MZ\x90\0"[..],
            b"\x7fELF",
            b"\xfe\xed\xfa\xcf",
            b"\xcf\xfa\xed\xfe",
            b"\xca\xfe\xba\xbe",
        ] {
            assert_eq!(sniff(program, "x.txt"), None);
        }
        assert_eq!(
            sniff(b"RIFF\0\0\0\0WAVEfmt ", "x"),
            None,
            "RIFF that isn't WebP"
        );
        // No `-`: not a PDF, just four bytes of text.
        assert_eq!(
            sniff(b"%PDF", "x"),
            Some(("txt", "text/plain; charset=utf-8"))
        );
        assert_eq!(sniff(b"", "x.txt"), None);
    }

    #[test]
    fn text_is_utf8_without_markup_or_scripts() {
        assert_eq!(
            sniff(b"a;b\n1;2\n", "Leads.CSV"),
            Some(("csv", "text/csv; charset=utf-8"))
        );
        assert_eq!(
            sniff("\u{feff}héllo # not a script".as_bytes(), "x"),
            Some(("txt", "text/plain; charset=utf-8"))
        );
        for bad in [
            &b"<svg/>"[..],
            b"  \n<!DOCTYPE html>",
            b"\xef\xbb\xbf<?xml version=\"1.0\"?>",
            b"x,<SCRIPT>",
            b"#!/usr/bin/env python",
            b"a\0b",
            b"caf\xe9",
        ] {
            assert_eq!(
                sniff(bad, "x.csv"),
                None,
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn broken_or_bogus_zips_are_unrecognised() {
        assert_eq!(sniff(b"PK\x03\x04", "x.zip"), None);
        // An end record pointing past itself.
        let mut b = b"PK\x03\x04".to_vec();
        b.extend_from_slice(b"PK\x05\x06\0\0\0\0\x01\0\x01\0\xff\xff\0\0\0\0\0\0\0\0");
        assert_eq!(sniff(&b, "x.zip"), None);
        // An empty archive is a zip.
        assert_eq!(
            sniff(b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0", "x.zip"),
            Some(("zip", "application/zip"))
        );
    }
}
