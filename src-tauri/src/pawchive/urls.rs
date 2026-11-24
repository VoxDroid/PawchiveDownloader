//! URLs of the supported sites' pages, API and media servers, and parsing of
//! the links people paste into the app.
//!
//! Pawchive, Kemono and Coomer run the same software. A service is written
//! bare for Pawchive (`patreon`) and with its site for the others
//! (`patreon@kemono`), so every key built from it stays unique and everything
//! saved before other sites existed keeps working.

use std::sync::LazyLock;

use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use super::types::RawFile;

pub const SITE_HOST: &str = "pawchive.pw";
pub const SITE_ORIGIN: &str = "https://pawchive.pw";

/// A site the app downloads from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Site {
    #[default]
    Pawchive,
    Kemono,
    Coomer,
}

impl Site {
    pub const ALL: [Site; 3] = [Site::Pawchive, Site::Kemono, Site::Coomer];

    pub fn id(self) -> &'static str {
        match self {
            Site::Pawchive => "pawchive",
            Site::Kemono => "kemono",
            Site::Coomer => "coomer",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Site::Pawchive => "Pawchive",
            Site::Kemono => "Kemono",
            Site::Coomer => "Coomer",
        }
    }
    pub fn parse(id: &str) -> Option<Site> {
        Site::ALL
            .into_iter()
            .find(|s| s.id() == id.trim().to_ascii_lowercase())
    }
    /// The host the app talks to.
    pub fn host(self) -> &'static str {
        match self {
            Site::Pawchive => "pawchive.pw",
            Site::Kemono => "kemono.cr",
            Site::Coomer => "coomer.st",
        }
    }
    /// Every domain the site used, current first (old links still open).
    fn domains(self) -> &'static [&'static str] {
        match self {
            Site::Pawchive => &["pawchive.pw"],
            Site::Kemono => &["kemono.cr", "kemono.su", "kemono.party"],
            Site::Coomer => &["coomer.st", "coomer.su", "coomer.party"],
        }
    }
    pub fn origin(self) -> String {
        format!("https://{}", self.host())
    }
    pub fn api_base(self) -> String {
        format!("https://{}/api/v1", self.host())
    }
    pub fn thumbnail_origin(self) -> String {
        format!("https://img.{}", self.host())
    }
    /// Where creator icons and banners live.
    fn image_origin(self) -> String {
        match self {
            Site::Pawchive => self.origin(),
            _ => self.thumbnail_origin(),
        }
    }
    /// The site a host belongs to, subdomains included (n1., img., www.).
    pub fn from_host(hostname: &str) -> Option<Site> {
        let host = hostname.trim_end_matches('.').to_ascii_lowercase();
        Site::ALL.into_iter().find(|s| {
            s.domains()
                .iter()
                .any(|d| host == *d || host.ends_with(&format!(".{d}")))
        })
    }
}

/// Splits `patreon@kemono` into the site and the bare service.
pub fn split_service(service: &str) -> (Site, &str) {
    match service.rsplit_once('@') {
        Some((bare, site)) => (Site::parse(site).unwrap_or_default(), bare),
        None => (Site::Pawchive, service),
    }
}

/// The service as the app stores it: bare on Pawchive, `bare@site` elsewhere.
pub fn qualify(site: Site, service: &str) -> String {
    let (_, bare) = split_service(service);
    match site {
        Site::Pawchive => bare.to_string(),
        other => format!("{bare}@{}", other.id()),
    }
}
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

/// True for any supported site and its subdomains (n1., img., t4., ...).
pub fn is_site_host(hostname: &str) -> bool {
    Site::from_host(hostname).is_some()
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
        #[serde(default)]
        site: Site,
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
    // Only the site itself, not its media or image servers.
    let Some(site) = Site::from_host(host).filter(|s| s.domains().contains(&host)) else {
        return PageRef::Other;
    };
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
        let service = qualify(site, &service.to_ascii_lowercase());
        if parts.len() >= 5 && parts[3] == "post" && ID_RE.is_match(&parts[4]) {
            return PageRef::Post {
                service,
                creator_id: creator_id.clone(),
                post_id: parts[4].clone(),
            };
        }
        if parts.len() == 3 {
            return PageRef::Creator {
                service,
                creator_id: creator_id.clone(),
                offset,
                query,
            };
        }
        return PageRef::Other;
    }
    if parts.len() == 1 && parts[0] == "posts" {
        return PageRef::Listing {
            offset,
            query,
            site,
        };
    }
    PageRef::Other
}

/// Every Pawchive link found in free text (one per line, or mixed with other
/// text), de-duplicated in order. Used by URL import and batch queueing.
pub fn extract_links(text: &str) -> Vec<(String, PageRef)> {
    static LINK_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r#"(?i)(?:https?://)?(?:www\.)?(?:pawchive\.pw|kemono\.(?:cr|su|party)|coomer\.(?:st|su|party))/[^\s<>"'`]+"#,
        )
        .unwrap()
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
            PageRef::Listing {
                offset,
                query,
                site,
            } => {
                format!(
                    "l:{}:{offset}:{}",
                    site.id(),
                    query.clone().unwrap_or_default()
                )
            }
            PageRef::Other => continue,
        };
        if seen.insert(key) {
            out.push((link.to_string(), parsed));
        }
    }
    out
}

/// Page URLs. Services may carry their site (`patreon@kemono`).
pub mod page {
    use super::{enc, split_service, Site};

    pub fn post(service: &str, creator_id: &str, post_id: &str) -> String {
        let (site, bare) = split_service(service);
        format!(
            "{}/{}/user/{}/post/{}",
            site.origin(),
            enc(bare),
            enc(creator_id),
            enc(post_id)
        )
    }
    pub fn creator(service: &str, creator_id: &str) -> String {
        let (site, bare) = split_service(service);
        format!("{}/{}/user/{}", site.origin(), enc(bare), enc(creator_id))
    }
    pub fn icon(service: &str, creator_id: &str) -> String {
        let (site, bare) = split_service(service);
        format!(
            "{}/icons/{}/{}",
            site.image_origin(),
            enc(bare),
            enc(creator_id)
        )
    }
    pub fn banner(service: &str, creator_id: &str) -> String {
        let (site, bare) = split_service(service);
        format!(
            "{}/banners/{}/{}",
            site.image_origin(),
            enc(bare),
            enc(creator_id)
        )
    }
    pub fn recent(site: Site) -> String {
        format!("{}/posts", site.origin())
    }
}

/// API URLs. Services may carry their site (`patreon@kemono`).
pub mod api {
    use super::{enc, split_service, Site};

    fn creator_base(service: &str, creator_id: &str) -> String {
        let (site, bare) = split_service(service);
        format!("{}/{}/user/{}", site.api_base(), enc(bare), enc(creator_id))
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
    pub fn creators(site: Site) -> String {
        format!("{}/creators", site.api_base())
    }
    pub fn linked(service: &str, creator_id: &str) -> String {
        format!("{}/links", creator_base(service, creator_id))
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
    pub fn recent_posts(site: Site, offset: u32, query: Option<&str>) -> String {
        paged(format!("{}/posts", site.api_base()), offset, query)
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

pub fn thumbnail_url(site: Site, path: &str) -> Option<String> {
    canonical_path(path).map(|c| format!("{}/thumbnail/data{c}", site.thumbnail_origin()))
}

/// Full-file URL for an API file entry, or None when only a preview exists.
/// pawchive.pw itself answers 404 for /data paths, so a Pawchive file needs
/// its media server; Kemono and Coomer send the request on from the site.
pub fn file_url(site: Site, file: &RawFile) -> Option<String> {
    let path = file.path.as_deref().filter(|p| !p.is_empty())?;
    if file.preview_only.unwrap_or(false) {
        return None;
    }
    let origin = match file.server.as_deref().and_then(|s| Url::parse(s).ok()) {
        Some(server)
            if server.scheme() == "https"
                && server.host_str().and_then(Site::from_host) == Some(site) =>
        {
            server.origin().ascii_serialization()
        }
        Some(_) => return None,
        None if site == Site::Pawchive => return None,
        None => site.origin(),
    };
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
                query: Some("dog".into()),
                site: Site::Pawchive,
            }
        );
        assert_eq!(
            parse_page_url("https://kemono.su/Fanbox/user/9/post/3"),
            PageRef::Post {
                service: "fanbox@kemono".into(),
                creator_id: "9".into(),
                post_id: "3".into()
            }
        );
        assert_eq!(
            parse_page_url("coomer.st/posts"),
            PageRef::Listing {
                offset: 0,
                query: None,
                site: Site::Coomer,
            }
        );
        assert_eq!(
            parse_page_url("https://n2.kemono.cr/patreon/user/1"),
            PageRef::Other
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
        assert_eq!(links.len(), 3);
        assert!(
            matches!(&links[2].1, PageRef::Creator { service, .. } if service == "patreon@kemono")
        );
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
            api::recent_posts(Site::Pawchive, 0, None),
            "https://pawchive.pw/api/v1/posts?o=0"
        );
        assert_eq!(
            api::profile("onlyfans@coomer", "x"),
            "https://coomer.st/api/v1/onlyfans/user/x/profile"
        );
        assert_eq!(
            page::icon("patreon@kemono", "5"),
            "https://img.kemono.cr/icons/patreon/5"
        );
        assert_eq!(
            page::icon("patreon", "5"),
            "https://pawchive.pw/icons/patreon/5"
        );
        assert_eq!(qualify(Site::Kemono, "patreon"), "patreon@kemono");
        assert_eq!(qualify(Site::Pawchive, "patreon@kemono"), "patreon");
        assert_eq!(split_service("fansly@coomer"), (Site::Coomer, "fansly"));
    }

    #[test]
    fn maps_storage_paths() {
        let hash = "64bb".to_string() + &"a".repeat(60);
        let path = format!("/264/bb/{hash}.PNG");
        assert_eq!(hash_from_path(&path), Some((hash.clone(), "png".into())));
        assert_eq!(canonical_path(&path), Some(format!("/64/bb/{hash}.png")));
        assert_eq!(
            thumbnail_url(Site::Pawchive, &path),
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
        let p = Site::Pawchive;
        assert_eq!(
            file_url(p, &f).as_deref(),
            Some("https://n1.pawchive.pw/data/26/bb/x.png?f=a%20b.png")
        );
        f.server = Some("https://evil.example".into());
        assert_eq!(file_url(p, &f), None);
        f.server = Some("https://n2.pawchive.pw".into());
        f.preview_only = Some(true);
        assert_eq!(file_url(p, &f), None);
        f.preview_only = None;
        f.server = None;
        assert_eq!(file_url(p, &f), None);
        // Kemono and Coomer send /data requests on to their media servers.
        assert_eq!(
            file_url(Site::Kemono, &f).as_deref(),
            Some("https://kemono.cr/data/26/bb/x.png?f=a%20b.png")
        );
        f.server = Some("https://n1.pawchive.pw".into());
        assert_eq!(file_url(Site::Kemono, &f), None);
    }

    /// Previews are shown straight from the sites, so the window's content
    /// security policy has to let every site's images in, media servers too.
    #[test]
    fn the_window_can_show_images_from_every_site() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).unwrap();
        let csp = conf["app"]["security"]["csp"].as_str().unwrap();
        let img = csp
            .split(';')
            .map(str::trim)
            .find(|d| d.starts_with("img-src"))
            .unwrap();
        let sources: Vec<&str> = img.split_whitespace().collect();
        for site in Site::ALL {
            for source in [
                format!("https://{}", site.host()),
                format!("https://*.{}", site.host()),
            ] {
                assert!(
                    sources.contains(&source.as_str()),
                    "{source} missing from {img}"
                );
            }
        }
    }
}
