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
        // Programs: ELF, Mach-O (both byte orders, fat). PE (`MZ`) always holds NUL bytes,
        // which `text` refuses, so text starting with "MZ" still passes.
        [0x7f, b'E', b'L', b'F', ..] => None,
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
    // Its comment must run to the end of the file (no decoy record inside a comment).
    if end + 22 + usize::from(u16_at(end + 20)?) != b.len() {
        return None;
    }
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
        // Part names compare without case (OPC); so do the program markers.
        let is = |n: &[u8]| name.eq_ignore_ascii_case(n);
        let starts = |p: &[u8]| {
            name.get(..p.len())
                .is_some_and(|h| h.eq_ignore_ascii_case(p))
        };
        let ends = |p: &[u8]| {
            name.len() >= p.len() && name[name.len() - p.len()..].eq_ignore_ascii_case(p)
        };
        if is(b"META-INF/MANIFEST.MF") || is(b"AndroidManifest.xml") || is(b"classes.dex") {
            return None;
        }
        types |= is(b"[Content_Types].xml");
        word |= starts(b"word/");
        xl |= starts(b"xl/");
        ppt |= starts(b"ppt/");
        macros |= ends(b"/vbaProject.bin");
        at += 46 + name_len + skip;
        if at > stop {
            return None;
        }
    }
    // The directory must hold exactly the entries the end record counts.
    if at != stop {
        return None;
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

/// Tags that make text a document a browser would render or run.
const MARKUP: &[&[u8]] = &[
    b"script",
    b"svg",
    b"html",
    b"!doctype",
    b"?xml",
    b"iframe",
    b"body",
    b"img",
    b"object",
    b"embed",
    b"meta",
    b"link",
    b"style",
    b"base",
    b"form",
];

/// UTF-8 text without NUL bytes that isn't markup (starts with `<`, or holds one of `MARKUP`
/// anywhere) or a script (`#!`); CSV when named so, else plain text.
fn text(b: &[u8], name: &str) -> Option<Kind> {
    let s = std::str::from_utf8(b).ok()?;
    let head = s.trim_start_matches('\u{feff}').trim_start();
    let markup = b.iter().enumerate().any(|(i, &c)| {
        c == b'<'
            && MARKUP.iter().any(|tag| {
                b.get(i + 1..i + 1 + tag.len())
                    .is_some_and(|w| w.eq_ignore_ascii_case(tag))
            })
    });
    if s.is_empty() || s.contains('\0') || head.starts_with('<') || head.starts_with("#!") || markup
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

    /// A stored zip of `names`, one byte each.
    fn zip(names: &[&str]) -> Vec<u8> {
        let (mut out, mut central) = (Vec::new(), Vec::new());
        for name in names {
            let offset = u32::try_from(out.len()).unwrap();
            let len = u16::try_from(name.len()).unwrap().to_le_bytes();
            out.extend_from_slice(b"PK\x03\x04\x14\0\0\0\0\0\0\0\0\0\0\0\0\0\x01\0\0\0\x01\0\0\0");
            out.extend_from_slice(&len);
            out.extend_from_slice(b"\0\0");
            out.extend_from_slice(name.as_bytes());
            out.push(b'x');
            central.extend_from_slice(
                b"PK\x01\x02\x14\0\x14\0\0\0\0\0\0\0\0\0\0\0\0\0\x01\0\0\0\x01\0\0\0",
            );
            central.extend_from_slice(&len);
            central.extend_from_slice(&[0; 12]);
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(name.as_bytes());
        }
        let start = u32::try_from(out.len()).unwrap();
        out.extend_from_slice(&central);
        let n = u16::try_from(names.len()).unwrap().to_le_bytes();
        out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
        out.extend_from_slice(&[n, n].concat());
        out.extend_from_slice(&u32::try_from(central.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(b"\0\0");
        out
    }

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
    fn text_may_start_like_a_program_header() {
        assert_eq!(
            sniff(b"MZ,amount\n1,2\n", "x.csv"),
            Some(("csv", "text/csv; charset=utf-8"))
        );
    }

    #[test]
    fn markup_anywhere_is_refused() {
        for bad in [
            &b"x<html><body onload=1>"[..],
            b"\x01<svg onload=1>",
            "\u{200B}<IMG src=x>".as_bytes(),
            b"a,b\n1,<iframe>",
        ] {
            assert_eq!(
                sniff(bad, "x.txt"),
                None,
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
        assert_eq!(
            sniff(b"a < b, <b>bold</b>", "x.txt"),
            Some(("txt", "text/plain; charset=utf-8"))
        );
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
        // A directory that holds more than the end record counts (a hidden JAR manifest).
        let mut hidden = zip(&["a.txt", "META-INF/MANIFEST.MF"]);
        let end = hidden.len() - 22;
        hidden[end + 8..end + 12].copy_from_slice(&[1, 0, 1, 0]);
        assert_eq!(sniff(&hidden, "x.zip"), None);
        // A comment length that doesn't match the file's end (a decoy end record).
        let mut decoy = zip(&["a.txt"]);
        let end = decoy.len() - 22;
        decoy[end + 20] = 5;
        assert_eq!(sniff(&decoy, "x.zip"), None);
        // Part names are case-insensitive: a VBAProject.bin still means macros.
        let docm = zip(&[
            "[Content_Types].xml",
            "word/document.xml",
            "word/VBAProject.bin",
        ]);
        assert_eq!(sniff(&docm, "x.docx").map(|k| k.0), Some("docm"));
        assert_eq!(
            sniff(&zip(&["a.txt"]), "x.zip"),
            Some(("zip", "application/zip"))
        );
        // An empty archive is a zip.
        assert_eq!(
            sniff(b"PK\x05\x06\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0", "x.zip"),
            Some(("zip", "application/zip"))
        );
    }
}
