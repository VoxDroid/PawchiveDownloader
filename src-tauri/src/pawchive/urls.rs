//! URLs of Pawchive's pages, API and media servers, and parsing of the links
//! people paste into the app.

use std::sync::LazyLock;

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use super::types::RawFile;

pub const SITE_HOST: &str = "pawchive.pw";
pub const SITE_ORIGIN: &str = "https://pawchive.pw";
pub const THUMBNAIL_ORIGIN: &str = "https://img.pawchive.pw";
pub const API_BASE: &str = "https://pawchive.pw/api/v1";
/// Posts per page on every paginated listing.
pub const PAGE_SIZE: u32 = 50;

static SERVICE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^[a-z0-9_-]{2,32}$").unwrap());
static ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\w.-]{1,128}$").unwrap());
static HASH_FILE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^([0-9a-f]{64})(?:\.([a-z0-9]{1,10}))?$").unwrap());

/// Characters kept as-is in a path segment (RFC 3986 unreserved).
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

pub fn enc(value: &str) -> String {
    utf8_percent_encode(value, SEGMENT).to_string()
}

/// True for pawchive.pw and any of its subdomains (n1., img., t4., ...).
pub fn is_site_host(hostname: &str) -> bool {
    let host = hostname.trim_end_matches('.').to_ascii_lowercase();
    host == SITE_HOST || host.ends_with(".pawchive.pw")
}

pub fn is_site_url(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .filter(|u| u.scheme() == "https")
        .and_then(|u| u.host_str().map(is_site_host))
        .unwrap_or(false)
}

/// What a Pawchive link points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PageRef {
    #[serde(rename_all = "camelCase")]
    Post {
        service: String,
        creator_id: String,
        post_id: String,
    },
    #[serde(rename_all = "camelCase")]
    Creator {
        service: String,
        creator_id: String,
        offset: u32,
        query: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Listing {
        offset: u32,
        query: Option<String>,
    },
    Other,
}

fn parse_offset(value: Option<String>) -> u32 {
    value
        .and_then(|v| v.parse::<u32>().ok())
        .map(|n| n - (n % PAGE_SIZE))
        .unwrap_or(0)
}

/// Works out what kind of Pawchive page a URL points at. Accepts links with or
/// without the scheme, and `www.` hosts.
pub fn parse_page_url(input: &str) -> PageRef {
    let text = input.trim();
    let candidate = if text.contains("://") {
        text.to_string()
    } else {
        format!("https://{text}")
    };
    let Ok(url) = Url::parse(&candidate) else {
        return PageRef::Other;
    };
    let Some(host) = url.host_str() else {
        return PageRef::Other;
    };
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    if host != SITE_HOST {
        return PageRef::Other;
    }
    let parts: Vec<String> = url
        .path_segments()
        .map(|s| {
            s.filter(|p| !p.is_empty())
                .map(|p| percent_decode_str(p).decode_utf8_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    let mut query = None;
    let mut offset = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "q" => {
                let v = v.trim().to_string();
                if !v.is_empty() {
                    query = Some(v);
                }
            }
            "o" => offset = Some(v.into_owned()),
            _ => {}
        }
    }
    let offset = parse_offset(offset);

    if parts.len() >= 3 && parts[1] == "user" {
        let service = &parts[0];
        let creator_id = &parts[2];
        if !SERVICE_RE.is_match(service) || !ID_RE.is_match(creator_id) {
            return PageRef::Other;
        }
        if parts.len() >= 5 && parts[3] == "post" && ID_RE.is_match(&parts[4]) {
            return PageRef::Post {
                service: service.clone(),
                creator_id: creator_id.clone(),
                post_id: parts[4].clone(),
            };
        }
        if parts.len() == 3 {
            return PageRef::Creator {
                service: service.clone(),
                creator_id: creator_id.clone(),
                offset,
                query,
            };
        }
        return PageRef::Other;
    }
    if parts.len() == 1 && parts[0] == "posts" {
        return PageRef::Listing { offset, query };
    }
    PageRef::Other
}

/// Every Pawchive link found in free text (one per line, or mixed with other
/// text), de-duplicated in order. Used by URL import and batch queueing.
pub fn extract_links(text: &str) -> Vec<(String, PageRef)> {
    static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)(?:https?://)?(?:www\.)?pawchive\.pw/[^\s<>"'`]+"#).unwrap()
    });
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for m in LINK_RE.find_iter(text) {
        let link = m.as_str().trim_end_matches(&[')', ']', ',', '.', ';'][..]);
        let parsed = parse_page_url(link);
        if parsed == PageRef::Other {
            continue;
        }
        let key = match &parsed {
            PageRef::Post {
                service,
                creator_id,
                post_id,
            } => format!("p:{service}:{creator_id}:{post_id}"),
            PageRef::Creator {
                service,
                creator_id,
                query,
                ..
            } => {
                format!(
                    "c:{service}:{creator_id}:{}",
                    query.clone().unwrap_or_default()
                )
            }
            PageRef::Listing { offset, query } => {
                format!("l:{offset}:{}", query.clone().unwrap_or_default())
            }
            PageRef::Other => continue,
        };
        if seen.insert(key) {
            out.push((link.to_string(), parsed));
        }
    }
    out
}

pub mod page {
    use super::{enc, SITE_ORIGIN};

    pub fn post(service: &str, creator_id: &str, post_id: &str) -> String {
        format!(
            "{SITE_ORIGIN}/{}/user/{}/post/{}",
            enc(service),
            enc(creator_id),
            enc(post_id)
        )
    }
    pub fn creator(service: &str, creator_id: &str) -> String {
        format!("{SITE_ORIGIN}/{}/user/{}", enc(service), enc(creator_id))
    }
    pub fn icon(service: &str, creator_id: &str) -> String {
        format!("{SITE_ORIGIN}/icons/{}/{}", enc(service), enc(creator_id))
    }
    pub fn banner(service: &str, creator_id: &str) -> String {
        format!("{SITE_ORIGIN}/banners/{}/{}", enc(service), enc(creator_id))
    }
}

pub mod api {
    use super::{enc, API_BASE};

    fn creator_base(service: &str, creator_id: &str) -> String {
        format!("{API_BASE}/{}/user/{}", enc(service), enc(creator_id))
    }
    fn paged(base: String, offset: u32, query: Option<&str>) -> String {
        let mut url = format!("{base}?o={offset}");
        if let Some(q) = query.filter(|q| !q.is_empty()) {
            url.push_str("&q=");
            url.push_str(&enc(q));
        }
        url
    }
    pub fn post(service: &str, creator_id: &str, post_id: &str) -> String {
        format!(
            "{}/post/{}",
            creator_base(service, creator_id),
            enc(post_id)
        )
    }
    pub fn profile(service: &str, creator_id: &str) -> String {
        format!("{}/profile", creator_base(service, creator_id))
    }
    pub fn comments(service: &str, creator_id: &str, post_id: &str) -> String {
        format!("{}/comments", post(service, creator_id, post_id))
    }
    pub fn announcements(service: &str, creator_id: &str) -> String {
        format!("{}/announcements", creator_base(service, creator_id))
    }
    pub fn creator_posts(
        service: &str,
        creator_id: &str,
        offset: u32,
        query: Option<&str>,
    ) -> String {
        paged(
            format!("{}/posts", creator_base(service, creator_id)),
            offset,
            query,
        )
    }
    pub fn recent_posts(offset: u32, query: Option<&str>) -> String {
        paged(format!("{API_BASE}/posts"), offset, query)
    }
}

/// The sha256 file name at the end of a storage path, if it has one.
pub fn hash_from_path(path: &str) -> Option<(String, String)> {
    let last = path.rsplit('/').next().unwrap_or("");
    let caps = HASH_FILE_RE.captures(last)?;
    Some((
        caps[1].to_ascii_lowercase(),
        caps.get(2)
            .map(|m| m.as_str().to_ascii_lowercase())
            .unwrap_or_default(),
    ))
}

/// Storage paths carry a server-specific prefix (`/264/bb/64bb...` on n1), but
/// thumbnails always live under the canonical `/64/bb/64bb...` layout.
pub fn canonical_path(path: &str) -> Option<String> {
    let (hash, ext) = hash_from_path(path)?;
    let ext = if ext.is_empty() {
        String::new()
    } else {
        format!(".{ext}")
    };
    Some(format!("/{}/{}/{hash}{ext}", &hash[0..2], &hash[2..4]))
}

pub fn thumbnail_url(path: &str) -> Option<String> {
    canonical_path(path).map(|c| format!("{THUMBNAIL_ORIGIN}/thumbnail/data{c}"))
}

/// Full-file URL for an API file entry, or None when only a preview exists.
/// pawchive.pw itself answers 404 for /data paths: without a media server
/// there is no full file to fetch.
pub fn file_url(file: &RawFile) -> Option<String> {
    let path = file.path.as_deref().filter(|p| !p.is_empty())?;
    if file.preview_only.unwrap_or(false) {
        return None;
    }
    let server = Url::parse(file.server.as_deref()?).ok()?;
    if server.scheme() != "https" || !server.host_str().map(is_site_host).unwrap_or(false) {
        return None;
    }
    let origin = server.origin().ascii_serialization();
    let path = if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let data_path = if path.starts_with("/data/") {
        path
    } else {
        format!("/data{path}")
    };
    let name = file
        .name
        .as_deref()
        .filter(|n| !n.is_empty())
        .map(|n| format!("?f={}", enc(n)))
        .unwrap_or_default();
    Some(format!("{origin}{data_path}{name}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_post_creator_and_listing_links() {
        assert_eq!(
            parse_page_url("https://pawchive.pw/patreon/user/123/post/456"),
            PageRef::Post {
                service: "patreon".into(),
                creator_id: "123".into(),
                post_id: "456".into()
            }
        );
        assert_eq!(
            parse_page_url("pawchive.pw/fanbox/user/9?o=75&q=cat"),
            PageRef::Creator {
                service: "fanbox".into(),
                creator_id: "9".into(),
                offset: 50,
                query: Some("cat".into())
            }
        );
        assert_eq!(
            parse_page_url("https://www.pawchive.pw/posts?q=%20dog%20"),
            PageRef::Listing {
                offset: 0,
                query: Some("dog".into())
            }
        );
        assert_eq!(
            parse_page_url("https://example.com/patreon/user/1"),
            PageRef::Other
        );
        assert_eq!(
            parse_page_url("https://n1.pawchive.pw/patreon/user/1"),
            PageRef::Other
        );
        assert_eq!(parse_page_url("not a url"), PageRef::Other);
        assert_eq!(
            parse_page_url("https://pawchive.pw/patreon/user/1/extra"),
            PageRef::Other
        );
    }

    #[test]
    fn extracts_unique_links_from_text() {
        let text = "see https://pawchive.pw/patreon/user/1 and (pawchive.pw/patreon/user/1/post/2).\n\
                    https://pawchive.pw/patreon/user/1\nhttps://kemono.cr/patreon/user/1\nhttps://pawchive.pw/about";
        let links = extract_links(text);
        assert_eq!(links.len(), 2);
        assert!(matches!(links[1].1, PageRef::Post { .. }));
        assert_eq!(links[1].0, "pawchive.pw/patreon/user/1/post/2");
    }

    #[test]
    fn builds_api_urls_with_encoding() {
        assert_eq!(
            api::creator_posts("patreon", "1", 50, Some("a b")),
            "https://pawchive.pw/api/v1/patreon/user/1/posts?o=50&q=a%20b"
        );
        assert_eq!(
            api::profile("fanbox", "x"),
            "https://pawchive.pw/api/v1/fanbox/user/x/profile"
        );
        assert_eq!(
            api::recent_posts(0, None),
            "https://pawchive.pw/api/v1/posts?o=0"
        );
    }

    #[test]
    fn maps_storage_paths() {
        let hash = "64bb".to_string() + &"a".repeat(60);
        let path = format!("/264/bb/{hash}.PNG");
        assert_eq!(hash_from_path(&path), Some((hash.clone(), "png".into())));
        assert_eq!(canonical_path(&path), Some(format!("/64/bb/{hash}.png")));
        assert_eq!(
            thumbnail_url(&path),
            Some(format!(
                "https://img.pawchive.pw/thumbnail/data/64/bb/{hash}.png"
            ))
        );
        assert_eq!(hash_from_path("/x/y/name.png"), None);
    }

    #[test]
    fn file_urls_need_a_site_media_server() {
        let mut f = RawFile {
            name: Some("a b.png".into()),
            path: Some("/26/bb/x.png".into()),
            server: Some("https://n1.pawchive.pw".into()),
            preview_only: None,
        };
        assert_eq!(
            file_url(&f).as_deref(),
            Some("https://n1.pawchive.pw/data/26/bb/x.png?f=a%20b.png")
        );
        f.server = Some("https://evil.example".into());
        assert_eq!(file_url(&f), None);
        f.server = Some("https://n2.pawchive.pw".into());
        f.preview_only = Some(true);
        assert_eq!(file_url(&f), None);
        f.preview_only = None;
        f.server = None;
        assert_eq!(file_url(&f), None);
    }
}
