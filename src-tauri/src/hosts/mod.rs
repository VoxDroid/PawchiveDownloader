//! Downloads from the file hosts creators share links to: Mega, Google Drive,
//! Dropbox, Pixeldrain, GoFile, MediaFire and plain file links.
//!
//! `resolve` turns a share link into the files behind it (a folder link
//! gives many); each comes with a direct URL, and Mega files with the key to
//! decrypt them, which never leaves this computer.

pub mod mega;

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use url::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    Mega,
    Drive,
    Dropbox,
    Pixeldrain,
    Gofile,
    Mediafire,
    /// A link straight to a file (catbox and similar).
    Direct,
}

impl Host {
    pub fn name(self) -> &'static str {
        match self {
            Host::Mega => "Mega",
            Host::Drive => "Google Drive",
            Host::Dropbox => "Dropbox",
            Host::Pixeldrain => "Pixeldrain",
            Host::Gofile => "GoFile",
            Host::Mediafire => "MediaFire",
            Host::Direct => "Direct",
        }
    }
}

/// One file behind a share link.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostFile {
    pub name: String,
    pub url: String,
    #[serde(default)]
    pub size: Option<u64>,
    /// Sub-folder inside a shared folder ("" at the top).
    #[serde(default)]
    pub folder: String,
    /// Extra request headers (GoFile needs its token as a cookie).
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    /// Mega's file key and nonce, to decrypt the download.
    #[serde(default)]
    pub mega: Option<mega::FileKey>,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum HostError {
    #[error("the link is not supported")]
    Unsupported,
    #[error("the file or folder is gone or private")]
    NotFound,
    #[error("the host refused: {0}")]
    Refused(String),
    #[error("{0}")]
    Network(String),
    #[error("cancelled")]
    Cancelled,
}

fn host_name(url: &Url) -> String {
    url.host_str()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_ascii_lowercase()
}

static FILE_EXT_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\.(zip|rar|7z|png|jpe?g|gif|webp|mp4|webm|mkv|mov|mp3|wav|flac|pdf|psd|clip|txt)$",
    )
    .unwrap()
});

/// Which host a link belongs to, when the app can download from it.
pub fn detect(link: &str) -> Option<Host> {
    let url = Url::parse(link).ok()?;
    let host = host_name(&url);
    let path = url.path();
    let in_domain = |d: &str| host == d || host.ends_with(&format!(".{d}"));
    if in_domain("mega.nz") || in_domain("mega.co.nz") || in_domain("mega.io") {
        return mega::parse_link(link).map(|_| Host::Mega);
    }
    if host == "drive.google.com"
        || host == "docs.google.com"
        || host == "drive.usercontent.google.com"
    {
        return (drive_file_id(&url).is_some() || drive_folder_id(&url).is_some())
            .then_some(Host::Drive);
    }
    if in_domain("dropbox.com") || host == "db.tt" {
        return Some(Host::Dropbox);
    }
    if in_domain("pixeldrain.com") {
        return pixeldrain_id(&url).map(|_| Host::Pixeldrain);
    }
    if in_domain("gofile.io") {
        return gofile_code(&url).map(|_| Host::Gofile);
    }
    if in_domain("mediafire.com") && (path.contains("/file/") || path.contains("/view/")) {
        return Some(Host::Mediafire);
    }
    if in_domain("catbox.moe") || FILE_EXT_RE.is_match(path) {
        return Some(Host::Direct);
    }
    None
}

fn drive_file_id(url: &Url) -> Option<String> {
    static FILE_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"/file/d/([\w-]{10,})").unwrap());
    if let Some(c) = FILE_RE.captures(url.path()) {
        return Some(c[1].to_string());
    }
    let path = url.path();
    if path.starts_with("/uc") || path.starts_with("/open") || path.starts_with("/download") {
        return url
            .query_pairs()
            .find(|(k, _)| k == "id")
            .map(|(_, v)| v.into_owned())
            .filter(|v| v.len() >= 10);
    }
    None
}

fn drive_folder_id(url: &Url) -> Option<String> {
    static FOLDER_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"/folders/([\w-]{10,})").unwrap());
    FOLDER_RE.captures(url.path()).map(|c| c[1].to_string())
}

fn pixeldrain_id(url: &Url) -> Option<(bool, String)> {
    let parts: Vec<&str> = url.path().split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        ["u", id, ..] => Some((false, id.to_string())),
        ["l", id, ..] => Some((true, id.to_string())),
        ["api", "file", id, ..] => Some((false, id.to_string())),
        _ => None,
    }
}

fn gofile_code(url: &Url) -> Option<String> {
    let parts: Vec<&str> = url.path().split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        ["d", code, ..] => Some(code.to_string()),
        _ => None,
    }
}

/// The file name a server offers in Content-Disposition, if any.
pub fn disposition_name(header: &str) -> Option<String> {
    static STAR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)filename\*\s*=\s*(?:UTF-8|utf-8)''([^;]+)"#).unwrap());
    static PLAIN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?i)filename\s*=\s*"?([^";]+)"?"#).unwrap());
    if let Some(c) = STAR.captures(header) {
        return percent_encoding::percent_decode_str(c[1].trim())
            .decode_utf8()
            .ok()
            .map(|s| s.into_owned());
    }
    PLAIN.captures(header).map(|c| c[1].trim().to_string())
}

fn last_segment(url: &Url) -> String {
    url.path_segments()
        .and_then(|mut s| s.next_back().map(str::to_string))
        .map(|s| {
            percent_encoding::percent_decode_str(&s)
                .decode_utf8_lossy()
                .into_owned()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "download".into())
}

async fn send(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
) -> Result<reqwest::Response, HostError> {
    let response = tokio::select! {
        r = request.send() => r.map_err(|e| HostError::Network(e.to_string()))?,
        _ = cancel.cancelled() => return Err(HostError::Cancelled),
    };
    match response.status().as_u16() {
        200..=299 => Ok(response),
        404 | 410 => Err(HostError::NotFound),
        s => Err(HostError::Refused(format!("HTTP {s}"))),
    }
}

async fn text(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
) -> Result<String, HostError> {
    let response = send(request, cancel).await?;
    tokio::select! {
        r = response.text() => r.map_err(|e| HostError::Network(e.to_string())),
        _ = cancel.cancelled() => Err(HostError::Cancelled),
    }
}

async fn json(
    request: reqwest::RequestBuilder,
    cancel: &CancellationToken,
) -> Result<serde_json::Value, HostError> {
    let body = text(request, cancel).await?;
    serde_json::from_str(&body).map_err(|_| HostError::Refused("unexpected answer".into()))
}

/// The files behind a share link.
pub async fn resolve(
    http: &reqwest::Client,
    link: &str,
    cancel: &CancellationToken,
) -> Result<Vec<HostFile>, HostError> {
    let host = detect(link).ok_or(HostError::Unsupported)?;
    let url = Url::parse(link).map_err(|_| HostError::Unsupported)?;
    match host {
        Host::Mega => mega::resolve(http, link, cancel).await,
        Host::Drive => drive(http, &url, cancel).await,
        Host::Dropbox => Ok(vec![dropbox(&url)]),
        Host::Pixeldrain => pixeldrain(http, &url, cancel).await,
        Host::Gofile => gofile(http, &url, cancel).await,
        Host::Mediafire => mediafire(http, &url, cancel).await,
        Host::Direct => Ok(vec![HostFile {
            name: last_segment(&url),
            url: link.to_string(),
            size: None,
            folder: String::new(),
            headers: vec![],
            mega: None,
        }]),
    }
}

/// `dl=1` makes Dropbox send the file itself (a folder comes as a ZIP).
fn dropbox(url: &Url) -> HostFile {
    let mut direct = url.clone();
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| k != "dl" && k != "raw")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    {
        let mut q = direct.query_pairs_mut();
        q.clear();
        for (k, v) in &pairs {
            q.append_pair(k, v);
        }
        q.append_pair("dl", "1");
    }
    let is_folder = url.path().contains("/fo/") || url.path().starts_with("/sh/");
    let name = if is_folder {
        format!("{}.zip", last_segment(url))
    } else {
        last_segment(url)
    };
    HostFile {
        name,
        url: direct.to_string(),
        size: None,
        folder: String::new(),
        headers: vec![],
        mega: None,
    }
}

async fn pixeldrain(
    http: &reqwest::Client,
    url: &Url,
    cancel: &CancellationToken,
) -> Result<Vec<HostFile>, HostError> {
    let (is_list, id) = pixeldrain_id(url).ok_or(HostError::Unsupported)?;
    let file = |id: &str, name: &str, size: Option<u64>| HostFile {
        name: name.to_string(),
        url: format!("https://pixeldrain.com/api/file/{id}?download"),
        size,
        folder: String::new(),
        headers: vec![],
        mega: None,
    };
    if is_list {
        let v = json(
            http.get(format!("https://pixeldrain.com/api/list/{id}")),
            cancel,
        )
        .await?;
        let files = v["files"].as_array().cloned().unwrap_or_default();
        return Ok(files
            .iter()
            .filter_map(|f| {
                let id = f["id"].as_str()?;
                Some(file(
                    id,
                    f["name"].as_str().unwrap_or(id),
                    f["size"].as_u64(),
                ))
            })
            .collect());
    }
    let info = json(
        http.get(format!("https://pixeldrain.com/api/file/{id}/info")),
        cancel,
    )
    .await?;
    Ok(vec![file(
        &id,
        info["name"].as_str().unwrap_or(&id),
        info["size"].as_u64(),
    )])
}

async fn drive(
    http: &reqwest::Client,
    url: &Url,
    cancel: &CancellationToken,
) -> Result<Vec<HostFile>, HostError> {
    if let Some(id) = drive_file_id(url) {
        return Ok(vec![drive_file(&id, None, "")]);
    }
    let folder = drive_folder_id(url).ok_or(HostError::Unsupported)?;
    let mut out = Vec::new();
    let mut queue = vec![(folder, String::new(), 0u8)];
    while let Some((id, path, depth)) = queue.pop() {
        let html = text(
            http.get(format!(
                "https://drive.google.com/embeddedfolderview?id={id}"
            )),
            cancel,
        )
        .await?;
        for entry in drive_folder_entries(&html) {
            match entry {
                DriveEntry::File { id, name } => out.push(drive_file(&id, Some(name), &path)),
                DriveEntry::Folder { id, name } if depth < 4 => {
                    let sub = if path.is_empty() {
                        name
                    } else {
                        format!("{path}/{name}")
                    };
                    queue.push((id, sub, depth + 1));
                }
                DriveEntry::Folder { .. } => {}
            }
            if out.len() >= 2000 {
                return Ok(out);
            }
        }
    }
    Ok(out)
}

fn drive_file(id: &str, name: Option<String>, folder: &str) -> HostFile {
    HostFile {
        // Named from the server's answer when the listing did not say.
        name: name.unwrap_or_default(),
        // confirm=t skips the "cannot scan for viruses" page of large files.
        url: format!(
            "https://drive.usercontent.google.com/download?id={id}&export=download&confirm=t"
        ),
        size: None,
        folder: folder.to_string(),
        headers: vec![],
        mega: None,
    }
}

#[derive(Debug, PartialEq)]
enum DriveEntry {
    File { id: String, name: String },
    Folder { id: String, name: String },
}

/// Entries of Drive's embedded folder view.
fn drive_folder_entries(html: &str) -> Vec<DriveEntry> {
    static ENTRY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?s)<a href="https://drive\.google\.com/(file/d|drive/folders)/([\w-]+)[^"]*".*?<div class="flip-entry-title">([^<]*)</div>"#,
        )
        .unwrap()
    });
    ENTRY_RE
        .captures_iter(html)
        .map(|c| {
            let name = html_unescape(c[3].trim());
            let id = c[2].to_string();
            if &c[1] == "file/d" {
                DriveEntry::File { id, name }
            } else {
                DriveEntry::Folder { id, name }
            }
        })
        .collect()
}

fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

async fn gofile(
    http: &reqwest::Client,
    url: &Url,
    cancel: &CancellationToken,
) -> Result<Vec<HostFile>, HostError> {
    let code = gofile_code(url).ok_or(HostError::Unsupported)?;
    // A guest account, as the website itself makes for every visitor.
    let account = json(http.post("https://api.gofile.io/accounts"), cancel).await?;
    let token = account["data"]["token"]
        .as_str()
        .ok_or_else(|| HostError::Refused("no guest account".into()))?
        .to_string();
    // The website token that GoFile's own page sends along.
    let script = text(http.get("https://gofile.io/dist/js/global.js"), cancel)
        .await
        .unwrap_or_default();
    static WT_RE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"wt\s*[:=]\s*"([^"]+)""#).unwrap());
    let wt = WT_RE
        .captures(&script)
        .map(|c| c[1].to_string())
        .unwrap_or_else(|| "4fd6sg89d7s6".into());
    let mut out = Vec::new();
    let mut queue = vec![(code, String::new(), 0u8)];
    while let Some((id, path, depth)) = queue.pop() {
        let v = json(
            http.get(format!("https://api.gofile.io/contents/{id}?wt={wt}"))
                .bearer_auth(&token),
            cancel,
        )
        .await?;
        if v["status"].as_str() != Some("ok") {
            return Err(match v["status"].as_str() {
                Some("error-notFound") => HostError::NotFound,
                Some(other) => HostError::Refused(other.to_string()),
                None => HostError::Refused("unexpected answer".into()),
            });
        }
        let data = &v["data"];
        let children = data["children"].as_object().cloned().unwrap_or_default();
        if children.is_empty() && data["type"] == "file" {
            children_push(&mut out, data, &path, &token);
        }
        for (_, child) in children {
            match child["type"].as_str() {
                Some("file") => children_push(&mut out, &child, &path, &token),
                Some("folder") if depth < 4 => {
                    let name = child["name"].as_str().unwrap_or("folder");
                    let sub = if path.is_empty() {
                        name.to_string()
                    } else {
                        format!("{path}/{name}")
                    };
                    if let Some(id) = child["id"].as_str() {
                        queue.push((id.to_string(), sub, depth + 1));
                    }
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

fn children_push(out: &mut Vec<HostFile>, f: &serde_json::Value, path: &str, token: &str) {
    if let Some(link) = f["link"].as_str() {
        out.push(HostFile {
            name: f["name"].as_str().unwrap_or("file").to_string(),
            url: link.to_string(),
            size: f["size"].as_u64(),
            folder: path.to_string(),
            headers: vec![("Cookie".into(), format!("accountToken={token}"))],
            mega: None,
        });
    }
}

async fn mediafire(
    http: &reqwest::Client,
    url: &Url,
    cancel: &CancellationToken,
) -> Result<Vec<HostFile>, HostError> {
    let html = text(http.get(url.as_str()), cancel).await?;
    static BUTTON_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?s)id="downloadButton"[^>]*href="(https://download[^"]+)""#).unwrap()
    });
    static BUTTON_RE2: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"href="(https://download\d*\.mediafire\.com/[^"]+)""#).unwrap()
    });
    let link = BUTTON_RE
        .captures(&html)
        .or_else(|| BUTTON_RE2.captures(&html))
        .map(|c| html_unescape(&c[1]))
        .ok_or(HostError::NotFound)?;
    let name = Url::parse(&link)
        .map(|u| last_segment(&u))
        .unwrap_or_else(|_| "download".into());
    Ok(vec![HostFile {
        name,
        url: link,
        size: None,
        folder: String::new(),
        headers: vec![],
        mega: None,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_hosts() {
        assert_eq!(
            detect("https://mega.nz/file/AbCd1234#kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
            Some(Host::Mega)
        );
        assert_eq!(
            detect("https://mega.nz/folder/AbCd1234#kkkkkkkkkkkkkkkkkkkkkk"),
            Some(Host::Mega)
        );
        assert_eq!(
            detect("https://drive.google.com/file/d/1AbCdEfGhIjK/view?usp=sharing"),
            Some(Host::Drive)
        );
        assert_eq!(
            detect("https://drive.google.com/drive/folders/1AbCdEfGhIjK"),
            Some(Host::Drive)
        );
        assert_eq!(
            detect("https://docs.google.com/document/d/1AbCdEfGhIjK"),
            None
        );
        assert_eq!(
            detect("https://www.dropbox.com/scl/fi/x/a.zip?rlkey=y&dl=0"),
            Some(Host::Dropbox)
        );
        assert_eq!(
            detect("https://pixeldrain.com/u/aBc123"),
            Some(Host::Pixeldrain)
        );
        assert_eq!(
            detect("https://pixeldrain.com/l/aBc123"),
            Some(Host::Pixeldrain)
        );
        assert_eq!(detect("https://gofile.io/d/AbC123"), Some(Host::Gofile));
        assert_eq!(
            detect("https://www.mediafire.com/file/abc/pack.zip/file"),
            Some(Host::Mediafire)
        );
        assert_eq!(
            detect("https://files.catbox.moe/abc.png"),
            Some(Host::Direct)
        );
        assert_eq!(detect("https://twitter.com/someone"), None);
    }

    #[test]
    fn makes_dropbox_links_direct() {
        let f = dropbox(
            &Url::parse("https://www.dropbox.com/scl/fi/abc/pack.zip?rlkey=k&dl=0").unwrap(),
        );
        assert_eq!(
            f.url,
            "https://www.dropbox.com/scl/fi/abc/pack.zip?rlkey=k&dl=1"
        );
        assert_eq!(f.name, "pack.zip");
        let folder =
            dropbox(&Url::parse("https://www.dropbox.com/scl/fo/abc/set?rlkey=k").unwrap());
        assert_eq!(folder.name, "set.zip");
    }

    #[test]
    fn reads_drive_ids_and_folder_views() {
        let u = Url::parse("https://drive.google.com/uc?id=1AbCdEfGhIjK&export=download").unwrap();
        assert_eq!(drive_file_id(&u).as_deref(), Some("1AbCdEfGhIjK"));
        let html = r#"<div class="flip-entry"><a href="https://drive.google.com/file/d/1FileIdAAAA/view?usp=drive_web" target="_blank"><div class="flip-entry-title">a &amp; b.png</div></a></div>
            <div class="flip-entry"><a href="https://drive.google.com/drive/folders/1FolderIdBB" target="_blank"><div class="flip-entry-title">Sub</div></a></div>"#;
        assert_eq!(
            drive_folder_entries(html),
            [
                DriveEntry::File {
                    id: "1FileIdAAAA".into(),
                    name: "a & b.png".into()
                },
                DriveEntry::Folder {
                    id: "1FolderIdBB".into(),
                    name: "Sub".into()
                },
            ]
        );
    }

    #[test]
    fn reads_disposition_names() {
        assert_eq!(
            disposition_name(r#"attachment; filename="a b.zip""#).as_deref(),
            Some("a b.zip")
        );
        assert_eq!(
            disposition_name("attachment; filename=\"x\"; filename*=UTF-8''%E7%8C%AB.png")
                .as_deref(),
            Some("\u{732b}.png")
        );
    }
}
