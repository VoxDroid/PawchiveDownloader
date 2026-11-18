//! File name sanitizing. Every path segment is cleaned so it is valid on
//! Windows, macOS and Linux, cannot hide its real extension and cannot point
//! outside the download folder.

use std::sync::LazyLock;

use regex::Regex;
use unicode_normalization::UnicodeNormalization;

pub const MAX_SEGMENT_LENGTH: usize = 120;
pub const MAX_PATH_LENGTH: usize = 240;
/// Linux and macOS limit a name to 255 bytes of UTF-8 (a Japanese character
/// takes 3), Windows to 255 UTF-16 units. 200 bytes leaves room for the
/// `.part` suffix and a ` (12)` number added later.
pub const MAX_SEGMENT_BYTES: usize = 200;

static RESERVED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(con|prn|aux|nul|com\d|lpt\d|conin\$|conout\$)$").unwrap());
// Extensions the shell treats as shortcuts or system hooks.
static SHELL_EXT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\.(lnk|local|scf|url|desktop|sys|cpl|cfg|ini)$").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());

fn is_invisible(c: char) -> bool {
    matches!(c as u32,
        0x0000..=0x001f | 0x007f..=0x009f | 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x2069 | 0xfeff)
}

fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn take_chars(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// The longest start of `s` within `max_chars` characters and `max_bytes`
/// bytes, never cutting a character in half.
fn take_within(s: &str, max_chars: usize, max_bytes: usize) -> &str {
    let mut end = 0;
    for (count, (i, c)) in s.char_indices().enumerate() {
        if count >= max_chars || i + c.len_utf8() > max_bytes {
            break;
        }
        end = i + c.len_utf8();
    }
    &s[..end]
}

/// Shortens a name to `max` characters and `MAX_SEGMENT_BYTES` bytes,
/// keeping a short extension.
pub fn truncate_keeping_extension(value: &str, max: usize) -> String {
    if char_len(value) <= max && value.len() <= MAX_SEGMENT_BYTES {
        return value.to_string();
    }
    let ext = match value.rfind('.') {
        Some(dot) if dot > 0 && char_len(&value[dot..]) <= 12 => &value[dot..],
        _ => "",
    };
    let stem = take_within(
        value,
        max.saturating_sub(char_len(ext)),
        MAX_SEGMENT_BYTES.saturating_sub(ext.len()),
    )
    .trim_end();
    format!("{stem}{ext}")
}

pub fn sanitize_segment(input: &str, fallback: &str) -> String {
    let mut value: String = input
        .nfc()
        .map(|c| {
            if matches!(c, '\t' | '\n' | '\r') {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !is_invisible(*c))
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect();
    value = SPACES.replace_all(&value, " ").trim().to_string();
    value = value
        .trim_start_matches(['.', '~', ' '])
        .trim_end_matches(['.', ' '])
        .to_string();
    if value.is_empty() {
        value = fallback.to_string();
    }
    let stem = value.split('.').next().unwrap_or(&value);
    if RESERVED.is_match(stem) {
        value = format!("_{value}");
    }
    if SHELL_EXT.is_match(&value) {
        value.push('_');
    }
    value = truncate_keeping_extension(&value, MAX_SEGMENT_LENGTH);
    let value = value.trim_end_matches(['.', ' ']).to_string();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

/// Cleans a relative path. Empty segments, `.` and `..` are dropped so a name
/// from the server can never point outside the download folder.
pub fn sanitize_path(input: &str) -> String {
    let segments: Vec<String> = input
        .split(['/', '\\'])
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .map(|s| sanitize_segment(s, "_"))
        .collect();
    if segments.is_empty() {
        return "_".into();
    }
    fit_path_length(segments)
}

/// Shortens the longest folder names first until the whole path fits.
fn fit_path_length(mut segments: Vec<String>) -> String {
    let total = |s: &Vec<String>| s.iter().map(|x| char_len(x)).sum::<usize>() + s.len() - 1;
    while total(&segments) > MAX_PATH_LENGTH {
        let last = segments.len() - 1;
        let mut longest = 0;
        for i in 1..last {
            if char_len(&segments[i]) > char_len(&segments[longest]) {
                longest = i;
            }
        }
        if segments.len() == 1 {
            longest = 0;
        }
        let current = segments[longest].clone();
        let excess = total(&segments) - MAX_PATH_LENGTH;
        let target = char_len(&current).saturating_sub(excess).max(8);
        if target >= char_len(&current) {
            // Folders are already minimal: shorten the file name itself.
            let len = char_len(&segments[last]);
            segments[last] =
                truncate_keeping_extension(&segments[last], len.saturating_sub(excess).max(16));
            break;
        }
        segments[longest] = if longest == last {
            truncate_keeping_extension(&current, target)
        } else {
            let cut = take_chars(&current, target).trim_end().to_string();
            if cut.is_empty() {
                "_".into()
            } else {
                cut
            }
        };
    }
    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_segments() {
        assert_eq!(sanitize_segment("a<b>:c\"d|e?f*g", "_"), "a_b__c_d_e_f_g");
        assert_eq!(sanitize_segment("  ..hidden. ", "_"), "hidden");
        assert_eq!(sanitize_segment("CON.txt", "_"), "_CON.txt");
        assert_eq!(sanitize_segment("evil\u{202e}gpj.exe", "_"), "evilgpj.exe");
        assert_eq!(sanitize_segment("link.lnk", "_"), "link.lnk_");
        assert_eq!(sanitize_segment("tab\tand\nnew", "_"), "tab and new");
        assert_eq!(sanitize_segment("...", "x"), "x");
        // Long CJK names stay within the byte limit of Linux and macOS.
        let cjk = sanitize_segment(&format!("{}.jpg", "\u{540d}".repeat(300)), "_");
        assert!(cjk.len() <= MAX_SEGMENT_BYTES, "{}", cjk.len());
        assert!(cjk.ends_with(".jpg"));
        assert_eq!(cjk.chars().count(), (MAX_SEGMENT_BYTES - 4) / 3 + 4);
        let folder = sanitize_segment(&"\u{9577}".repeat(200), "_");
        assert!(folder.len() <= MAX_SEGMENT_BYTES);
        let long = format!("{}.png", "a".repeat(200));
        let out = sanitize_segment(&long, "_");
        assert_eq!(out.chars().count(), MAX_SEGMENT_LENGTH);
        assert!(out.ends_with(".png"));
    }

    #[test]
    fn never_escapes_the_root() {
        assert_eq!(sanitize_path("../../etc/passwd"), "etc/passwd");
        assert_eq!(sanitize_path("a\\..\\b//c"), "a/b/c");
        assert_eq!(sanitize_path("/"), "_");
    }

    #[test]
    fn fits_long_paths() {
        let path = format!(
            "{}/{}/{}.jpg",
            "f".repeat(110),
            "g".repeat(110),
            "n".repeat(100)
        );
        let out = sanitize_path(&path);
        assert!(out.chars().count() <= MAX_PATH_LENGTH, "{}", out.len());
        assert!(out.ends_with(".jpg"));
        let unicode = format!("{}/{}.png", "日".repeat(130), "本".repeat(130));
        assert!(sanitize_path(&unicode).chars().count() <= MAX_PATH_LENGTH);
    }
}
