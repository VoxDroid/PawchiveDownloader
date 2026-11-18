//! Path templates such as `Pawchive/{creator}/{published:YYYY-MM-DD} {title}`.
//!
//! Tokens: {service} {creator} {creator_id} {post_id} {title} {published}
//! {added} {date} {index} {count} {name} {filename} {ext} {hash} {role}.
//! Date tokens take a format after a colon (YYYY YY MM DD HH mm ss), {index}
//! takes a zero-padding width ({index:3} -> 007) and {hash} a length.

use std::sync::LazyLock;

use chrono::{DateTime, Datelike, Local, NaiveDateTime, Timelike, Utc};
use regex::Regex;
use serde::Serialize;

use super::sanitize::{sanitize_path, sanitize_segment};

#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameContext {
    pub service: String,
    pub creator_id: String,
    pub creator_name: String,
    pub post_id: String,
    pub post_title: String,
    pub published: Option<String>,
    pub added: Option<String>,
    /// 1-based position of the file in the post.
    pub index: u32,
    /// Number of files in the post.
    pub count: u32,
    /// Original file name including extension.
    pub file_name: String,
    pub ext: String,
    pub hash: Option<String>,
    pub role: String,
    /// When the download was queued (local time), as epoch ms.
    #[serde(default)]
    pub now_ms: Option<i64>,
}

pub const TEMPLATE_TOKENS: [&str; 15] = [
    "service",
    "creator",
    "creator_id",
    "post_id",
    "title",
    "published",
    "added",
    "date",
    "index",
    "count",
    "name",
    "filename",
    "ext",
    "hash",
    "role",
];

static TOKEN_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\{([a-z_]+)(?::([^{}]*))?\}").unwrap());

pub const DEFAULT_FOLDER_TEMPLATE: &str =
    "Pawchive/{creator} ({service})/{published:YYYY-MM-DD} {title} [{post_id}]";
pub const DEFAULT_FILE_TEMPLATE: &str = "{index:2}_{filename}";
pub const DEFAULT_ARCHIVE_TEMPLATE: &str = "{creator} - {published:YYYY-MM-DD} {title} [{post_id}]";

/// Parses the API's naive timestamps (`2026-11-17T08:59:08`) as UTC.
fn parse_date(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(d) = DateTime::parse_from_rfc3339(value) {
        return Some(d.with_timezone(&Utc));
    }
    let normalized = value.replace(' ', "T");
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(d) = NaiveDateTime::parse_from_str(&normalized, fmt) {
            return Some(d.and_utc());
        }
    }
    chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .map(|d| d.and_hms_opt(0, 0, 0).unwrap().and_utc())
}

pub fn format_date<T: Datelike + Timelike>(date: &T, format: &str) -> String {
    static PARTS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"YYYY|YY|MM|DD|HH|mm|ss").unwrap());
    PARTS
        .replace_all(format, |c: &regex::Captures| match &c[0] {
            "YYYY" => date.year().to_string(),
            "YY" => format!("{:02}", date.year().rem_euclid(100)),
            "MM" => format!("{:02}", date.month()),
            "DD" => format!("{:02}", date.day()),
            "HH" => format!("{:02}", date.hour()),
            "mm" => format!("{:02}", date.minute()),
            _ => format!("{:02}", date.second()),
        })
        .into_owned()
}

fn stem(file_name: &str, ext: &str) -> String {
    let suffix = format!(".{}", ext.to_lowercase());
    if !ext.is_empty() && file_name.to_lowercase().ends_with(&suffix) {
        file_name[..file_name.len() - suffix.len()].to_string()
    } else {
        file_name.to_string()
    }
}

fn token_value(token: &str, arg: Option<&str>, ctx: &NameContext) -> Option<String> {
    let arg = arg.filter(|a| !a.is_empty());
    Some(match token.to_ascii_lowercase().as_str() {
        "service" => ctx.service.clone(),
        "creator" => {
            if ctx.creator_name.is_empty() {
                ctx.creator_id.clone()
            } else {
                ctx.creator_name.clone()
            }
        }
        "creator_id" => ctx.creator_id.clone(),
        "post_id" => ctx.post_id.clone(),
        "title" => {
            if ctx.post_title.is_empty() {
                ctx.post_id.clone()
            } else {
                ctx.post_title.clone()
            }
        }
        "published" => parse_date(ctx.published.as_deref())
            .map(|d| format_date(&d, arg.unwrap_or("YYYY-MM-DD")))
            .unwrap_or_default(),
        "added" => parse_date(ctx.added.as_deref())
            .map(|d| format_date(&d, arg.unwrap_or("YYYY-MM-DD")))
            .unwrap_or_default(),
        "date" => {
            let now = ctx
                .now_ms
                .and_then(DateTime::from_timestamp_millis)
                .map(|d| d.with_timezone(&Local))
                .unwrap_or_else(Local::now);
            format_date(&now, arg.unwrap_or("YYYY-MM-DD"))
        }
        "index" => {
            let auto = ctx.count.to_string().len();
            let width = arg
                .and_then(|a| a.parse::<usize>().ok())
                .map(|w| w.clamp(1, 8))
                .unwrap_or(auto);
            format!("{:0width$}", ctx.index, width = width)
        }
        "count" => ctx.count.to_string(),
        "name" => stem(&ctx.file_name, &ctx.ext),
        "filename" => ctx.file_name.clone(),
        "ext" => ctx.ext.clone(),
        "hash" => {
            let hash = ctx.hash.clone().unwrap_or_default();
            match arg.and_then(|a| a.parse::<usize>().ok()).filter(|n| *n > 0) {
                Some(n) => hash.chars().take(n).collect(),
                None => hash,
            }
        }
        "role" => ctx.role.clone(),
        _ => return None,
    })
}

/// Replaces tokens; each value is made path-safe so it cannot add folders.
pub fn render_template(template: &str, ctx: &NameContext) -> String {
    TOKEN_RE
        .replace_all(template, |c: &regex::Captures| {
            match token_value(&c[1], c.get(2).map(|m| m.as_str()), ctx) {
                None => c[0].to_string(),
                Some(v) if v.is_empty() => String::new(),
                Some(v) => sanitize_segment(&v, "").replace('/', "_"),
            }
        })
        .into_owned()
}

pub fn unknown_tokens(template: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for c in TOKEN_RE.captures_iter(template) {
        let token = c[1].to_ascii_lowercase();
        if !TEMPLATE_TOKENS.contains(&token.as_str()) && !out.contains(&c[1].to_string()) {
            out.push(c[1].to_string());
        }
    }
    out
}

/// Problems with a template, as a short code the UI translates.
pub fn validate_template(template: &str, kind: &str) -> Option<&'static str> {
    static PARENT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(^|[\\/])\.\.([\\/]|$)").unwrap());
    if template.trim().is_empty() {
        return if kind == "file" { Some("empty") } else { None };
    }
    if PARENT.is_match(template) {
        return Some("parent");
    }
    if !unknown_tokens(template).is_empty() {
        return Some("unknown");
    }
    if template.matches('{').count() != template.matches('}').count() {
        return Some("braces");
    }
    None
}

/// Folder + file name for one file, relative to the download folder.
pub fn build_file_path(folder_template: &str, file_template: &str, ctx: &NameContext) -> String {
    let folder = render_template(folder_template, ctx);
    let template = if file_template.is_empty() {
        DEFAULT_FILE_TEMPLATE
    } else {
        file_template
    };
    let mut file = render_template(template, ctx)
        .replace(['\\', '/'], "_")
        .trim()
        .to_string();
    if file.is_empty() || file == "." || file == format!(".{}", ctx.ext) {
        file = if ctx.file_name.is_empty() {
            format!("{}_{}", ctx.post_id, ctx.index)
        } else {
            ctx.file_name.clone()
        };
    }
    if !ctx.ext.is_empty()
        && !file
            .to_lowercase()
            .ends_with(&format!(".{}", ctx.ext.to_lowercase()))
    {
        file = format!("{file}.{}", ctx.ext);
    }
    sanitize_path(&if folder.is_empty() {
        file
    } else {
        format!("{folder}/{file}")
    })
}

pub fn build_folder_path(folder_template: &str, ctx: &NameContext) -> String {
    let folder = render_template(folder_template, ctx);
    if folder.trim().is_empty() {
        String::new()
    } else {
        sanitize_path(&folder)
    }
}

/// Name of a post's ZIP archive (folder template + archive template + .zip).
/// The archive replaces the post's own folder, so the last folder level is
/// dropped when it is the per-post one.
pub fn build_archive_path(
    folder_template: &str,
    archive_template: &str,
    ctx: &NameContext,
) -> String {
    static POST_LEVEL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\{(post_id|title|published|added)").unwrap());
    let segments: Vec<&str> = folder_template.split('/').collect();
    let post_level = POST_LEVEL.is_match(segments.last().copied().unwrap_or(""));
    let parent = if post_level {
        segments[..segments.len() - 1].join("/")
    } else {
        folder_template.to_string()
    };
    let folder = render_template(&parent, ctx);
    let template = if archive_template.is_empty() {
        DEFAULT_ARCHIVE_TEMPLATE
    } else {
        archive_template
    };
    let mut name = render_template(template, ctx).replace(['\\', '/'], "_");
    if name.is_empty() {
        name = ctx.post_id.clone();
    }
    sanitize_path(&format!("{folder}/{name}.zip"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> NameContext {
        NameContext {
            service: "patreon".into(),
            creator_id: "42".into(),
            creator_name: "Aster: Vale".into(),
            post_id: "777".into(),
            post_title: "Forest / Studies".into(),
            published: Some("2026-03-04T05:06:07".into()),
            added: None,
            index: 7,
            count: 12,
            file_name: "Pic One.PNG".into(),
            ext: "png".into(),
            hash: Some("abcdef0123".into()),
            role: "attachment".into(),
            now_ms: Some(0),
        }
    }

    #[test]
    fn renders_tokens() {
        let c = ctx();
        assert_eq!(
            render_template("{creator}|{title}", &c),
            "Aster_ Vale|Forest _ Studies"
        );
        assert_eq!(
            render_template("{published:YYYY/MM} {published:YY-DD HH.mm.ss}", &c),
            "2026_03 26-04 05.06.07"
        );
        assert_eq!(
            render_template("{index} {index:4} {count} {hash:4} {role}", &c),
            "07 0007 12 abcd attachment"
        );
        assert_eq!(
            render_template("{name}.{ext} {filename} {added} {nope}", &c),
            "Pic One.png Pic One.PNG  {nope}"
        );
    }

    #[test]
    fn builds_paths() {
        let c = ctx();
        assert_eq!(
            build_file_path(DEFAULT_FOLDER_TEMPLATE, DEFAULT_FILE_TEMPLATE, &c),
            "Pawchive/Aster_ Vale (patreon)/2026-03-04 Forest _ Studies [777]/07_Pic One.PNG"
        );
        assert_eq!(build_file_path("", "{hash}", &c), "abcdef0123.png");
        assert_eq!(build_file_path("", "{added}", &c), "Pic One.PNG");
        assert_eq!(build_folder_path("{creator}/../x", &c), "Aster_ Vale/x");
        assert_eq!(
            build_archive_path(DEFAULT_FOLDER_TEMPLATE, DEFAULT_ARCHIVE_TEMPLATE, &c),
            "Pawchive/Aster_ Vale (patreon)/Aster_ Vale - 2026-03-04 Forest _ Studies [777].zip"
        );
        assert_eq!(build_archive_path("Flat", "{post_id}", &c), "Flat/777.zip");
    }

    #[test]
    fn validates_templates() {
        assert_eq!(validate_template("", "file"), Some("empty"));
        assert_eq!(validate_template("", "folder"), None);
        assert_eq!(validate_template("a/../b", "folder"), Some("parent"));
        assert_eq!(validate_template("{wat}", "file"), Some("unknown"));
        assert_eq!(validate_template("{title", "file"), Some("braces"));
        assert_eq!(validate_template(DEFAULT_FOLDER_TEMPLATE, "folder"), None);
        assert_eq!(unknown_tokens("{a}{B}{a}{title}"), vec!["a", "B"]);
    }

    #[test]
    fn parses_api_dates() {
        assert!(parse_date(Some("2026-11-17T08:59:08")).is_some());
        assert!(parse_date(Some("2026-11-17 08:59:08.123")).is_some());
        assert!(parse_date(Some("2026-11-17T08:59:08+02:00")).is_some());
        assert!(parse_date(Some("2026-11-17")).is_some());
        assert!(parse_date(Some("garbage")).is_none());
    }
}
