//! Turns API answers into the app's models.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use url::Url;

use super::types::*;
use super::urls::{file_url, hash_from_path, is_site_url, page, thumbnail_url};

pub fn extension_of(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("");
    match base.rfind('.') {
        Some(dot) if dot > 0 && dot < base.len() - 1 => base[dot + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn category_of(ext: &str) -> FileCategory {
    match ext.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "bmp" | "tif" | "tiff" | "jfif"
        | "heic" | "psd" | "clip" | "svg" => FileCategory::Image,
        "mp4" | "m4v" | "webm" | "mkv" | "mov" | "avi" | "wmv" | "flv" | "mpg" | "mpeg" | "ts" => {
            FileCategory::Video
        }
        "mp3" | "wav" | "flac" | "ogg" | "oga" | "m4a" | "aac" | "opus" | "wma" => {
            FileCategory::Audio
        }
        "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "lzh" => {
            FileCategory::Archive
        }
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "epub" | "rtf" => {
            FileCategory::Document
        }
        "txt" | "md" | "json" | "csv" | "html" | "htm" | "xml" | "srt" | "ass" | "vtt" => {
            FileCategory::Text
        }
        _ => FileCategory::Other,
    }
}

/// Thumbnails exist only for these; the server answers 404 for anything else.
fn has_thumbnail(ext: &str) -> bool {
    matches!(
        ext,
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "bmp" | "jfif"
    )
}

fn build_file(raw: &RawFile, role: FileRole, index: u32) -> Option<PostFile> {
    let path = raw.path.clone().filter(|p| !p.is_empty())?;
    let info = hash_from_path(&path);
    let name = raw
        .name
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .or_else(|| {
            path.rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| format!("file-{index}"));
    let mut ext = extension_of(&name);
    if ext.is_empty() {
        ext = info.as_ref().map(|i| i.1.clone()).unwrap_or_default();
    }
    let url = file_url(raw);
    Some(PostFile {
        index,
        role,
        thumbnail_url: if has_thumbnail(&ext) {
            thumbnail_url(&path)
        } else {
            None
        },
        preview_only: url.is_none(),
        category: category_of(&ext),
        hash: info.map(|i| i.0),
        name,
        ext,
        path,
        url,
    })
}

static IMG_SRC_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?is)<img\b[^>]*?\bsrc\s*=\s*(["'])(.*?)["']"#).unwrap());

pub fn decode_entities(value: &str) -> String {
    static NUM: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#(\d+);").unwrap());
    let text = value
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    let text = NUM
        .replace_all(&text, |c: &regex::Captures| {
            c[1].parse::<u32>()
                .ok()
                .and_then(char::from_u32)
                .map(String::from)
                .unwrap_or_default()
        })
        .into_owned();
    text.replace("&amp;", "&")
}

/// Images embedded in the post body that live on Pawchive's own servers.
pub fn inline_images(content_html: &str) -> Vec<RawFile> {
    let mut found = Vec::new();
    for caps in IMG_SRC_RE.captures_iter(content_html) {
        let src = decode_entities(caps.get(2).map(|m| m.as_str()).unwrap_or(""))
            .trim()
            .to_string();
        if src.is_empty() {
            continue;
        }
        if src.starts_with('/') {
            if hash_from_path(src.split('?').next().unwrap_or("")).is_none() {
                continue;
            }
            let path = src
                .trim_start_matches("/data")
                .split('?')
                .next()
                .unwrap_or("")
                .to_string();
            let name = path.rsplit('/').next().map(str::to_string);
            found.push(RawFile {
                path: Some(path),
                name,
                server: None,
                preview_only: Some(true),
            });
            continue;
        }
        if !is_site_url(&src) {
            continue;
        }
        let Ok(url) = Url::parse(&src) else { continue };
        let path = url.path().trim_start_matches("/data").to_string();
        if hash_from_path(&path).is_none() {
            continue;
        }
        let name = url
            .query_pairs()
            .find(|(k, _)| k == "f")
            .map(|(_, v)| v.into_owned())
            .or_else(|| path.rsplit('/').next().map(str::to_string));
        found.push(RawFile {
            path: Some(path),
            name,
            server: Some(url.origin().ascii_serialization()),
            preview_only: None,
        });
    }
    found
}

fn id_of(value: &Option<serde_json::Value>) -> Option<String> {
    match value {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

/// The API repeats the main file among the attachments, and the body may embed
/// the same images again, so files are de-duplicated by content hash (or path).
pub fn normalize_post(raw: RawPost) -> Post {
    let content_html = raw
        .content
        .clone()
        .or(raw.substring.clone())
        .unwrap_or_default();
    let mut candidates: Vec<(RawFile, FileRole)> = Vec::new();
    if let Some(file) = raw
        .file
        .as_ref()
        .filter(|f| f.path.as_deref().is_some_and(|p| !p.is_empty()))
    {
        candidates.push((file.clone(), FileRole::File));
    }
    for a in raw.attachments.iter().flatten() {
        candidates.push((a.clone(), FileRole::Attachment));
    }
    for i in inline_images(&content_html) {
        candidates.push((i, FileRole::Inline));
    }
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    for (raw_file, role) in candidates {
        let Some(file) = build_file(&raw_file, role, files.len() as u32 + 1) else {
            continue;
        };
        let key = file.hash.clone().unwrap_or_else(|| file.path.clone());
        if !seen.insert(key) {
            continue;
        }
        files.push(file);
    }
    let incomplete = raw
        .attachments
        .iter()
        .flatten()
        .any(|a| a.path.as_deref().is_none_or(str::is_empty));
    let tags = match &raw.tags {
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    Post {
        url: page::post(&raw.service, &raw.user, &raw.id),
        id: raw.id,
        service: raw.service,
        creator_id: raw.user,
        title: raw.title.unwrap_or_default().trim().to_string(),
        content_html,
        published: raw.published.filter(|s| !s.is_empty()),
        added: raw.added.filter(|s| !s.is_empty()),
        edited: raw.edited.filter(|s| !s.is_empty()),
        tags,
        files,
        next_id: id_of(&raw.next),
        prev_id: id_of(&raw.prev),
        incomplete,
    }
}

fn to_iso(value: &Option<serde_json::Value>) -> Option<String> {
    match value {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => n
            .as_f64()
            .and_then(|secs| chrono::DateTime::from_timestamp(secs as i64, 0))
            .map(|d| d.format("%Y-%m-%dT%H:%M:%S").to_string()),
        _ => None,
    }
}

pub fn normalize_creator(raw: RawProfile) -> Creator {
    Creator {
        name: raw
            .name
            .as_deref()
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .unwrap_or(&raw.id)
            .to_string(),
        url: page::creator(&raw.service, &raw.id),
        icon_url: page::icon(&raw.service, &raw.id),
        banner_url: page::banner(&raw.service, &raw.id),
        updated: to_iso(&raw.updated),
        public_id: raw.public_id,
        post_count: raw.post_count,
        id: raw.id,
        service: raw.service,
    }
}

/// Converts post body HTML into readable plain text for `content.txt`.
pub fn html_to_text(html: &str) -> String {
    static BR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<\s*br\s*/?>").unwrap());
    static BLOCK_END: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)<\s*/\s*(p|div|h[1-6]|li|blockquote|pre|tr)\s*>").unwrap()
    });
    static LI: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<\s*li\b[^>]*>").unwrap());
    static LINK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<a\b[^>]*?href\s*=\s*["']([^"']*)["'][^>]*>(.*?)</a>"#).unwrap()
    });
    static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
    static GAPS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").unwrap());

    let text = html.replace("\r\n", "\n").replace('\r', "\n");
    let text = BR.replace_all(&text, "\n");
    let text = BLOCK_END.replace_all(&text, "\n");
    let text = LI.replace_all(&text, "- ");
    let text = LINK.replace_all(&text, |c: &regex::Captures| {
        let href = &c[1];
        let inner = TAG.replace_all(&c[2], "");
        if !inner.is_empty() && inner != href {
            format!("{inner} ({href})")
        } else {
            href.to_string()
        }
    });
    let text = TAG.replace_all(&text, "");
    let text = decode_entities(&text.replace("&nbsp;", " "));
    let joined = text
        .split('\n')
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    GAPS.replace_all(&joined, "\n\n").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(c: char) -> String {
        std::iter::repeat_n(c, 64).collect()
    }

    fn raw(json: serde_json::Value) -> RawPost {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn dedupes_main_file_attachments_and_inline_images() {
        let a = hash('a');
        let b = hash('b');
        let post = normalize_post(raw(serde_json::json!({
            "id": 7, "user": "42", "service": "patreon", "title": "  Hello  ",
            "content": format!("<p>Hi</p><img src=\"https://n2.pawchive.pw/data/bb/bb/{b}.jpg?f=pic.jpg\">"),
            "file": {"name": "main.png", "path": format!("/aa/aa/{a}.png"), "server": "https://n1.pawchive.pw"},
            "attachments": [
                {"name": "main.png", "path": format!("/aa/aa/{a}.png"), "server": "https://n1.pawchive.pw"},
                {"name": "clip.mp4", "path": "/cc/cc/clip.mp4", "preview_only": true}
            ],
            "tags": ["x", 1], "next": 8
        })));
        assert_eq!(post.id, "7");
        assert_eq!(post.title, "Hello");
        assert_eq!(post.tags, vec!["x"]);
        assert_eq!(post.next_id.as_deref(), Some("8"));
        assert_eq!(post.files.len(), 3);
        assert_eq!(post.files[0].role, FileRole::File);
        assert_eq!(post.files[0].hash.as_deref(), Some(a.as_str()));
        assert!(post.files[0].thumbnail_url.is_some());
        assert!(post.files[1].preview_only);
        assert_eq!(post.files[1].category, FileCategory::Video);
        assert!(post.files[1].thumbnail_url.is_none());
        assert_eq!(post.files[2].role, FileRole::Inline);
        assert_eq!(post.files[2].name, "pic.jpg");
        assert_eq!(post.files[2].index, 3);
        assert!(!post.incomplete);
        let listed = normalize_post(raw(serde_json::json!({
            "id": 1, "user": 2, "service": "patreon", "attachments": [{"name": "v.mp4", "deferred": true}]
        })));
        assert!(listed.incomplete);
        assert!(listed.files.is_empty());
    }

    #[test]
    fn categorises_extensions() {
        assert_eq!(extension_of("a/b.TAR.GZ"), "gz");
        assert_eq!(extension_of(".hidden"), "");
        assert_eq!(category_of("PSD"), FileCategory::Image);
        assert_eq!(category_of("weird"), FileCategory::Other);
    }

    #[test]
    fn converts_html_to_text() {
        let text = html_to_text("<p>One &amp; two</p><ul><li>a</li><li>b</li></ul><a href=\"https://x.y\">site</a><br>end&nbsp;");
        assert_eq!(text, "One & two\n- a\n- b\nsite (https://x.y)\nend");
    }

    #[test]
    fn normalizes_creators() {
        let c = normalize_creator(
            serde_json::from_value(serde_json::json!({
                "id": "5", "name": " ", "service": "fanbox", "updated": 1700000000
            }))
            .unwrap(),
        );
        assert_eq!(c.name, "5");
        assert_eq!(c.updated.as_deref(), Some("2023-11-14T22:13:20"));
        assert_eq!(c.icon_url, "https://pawchive.pw/icons/fanbox/5");
    }
}
