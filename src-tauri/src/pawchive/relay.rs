//! Preview-only files have no permanent URL. The post page instead links them
//! to a signed relay (`https://t4.pawchive.pw/f/<id>/<name>?e=<expiry>&s=<sig>`)
//! that serves the full file for a limited time. Only the HTML page carries
//! these links, so they are scraped from it.

use std::collections::HashMap;
use std::sync::LazyLock;

use percent_encoding::percent_decode_str;
use regex::Regex;
use url::Url;

use super::normalize::decode_entities;
use super::types::RelayLink;
use super::urls::{canonical_path, is_site_host};

static ANCHOR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<a\b([^>]*)>(.*?)</a>").unwrap());
static ATTR_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"([:\w-]+)\s*=\s*("([^"]*)"|'([^']*)'|([^\s>]+))"#).unwrap());
static IMG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<img\b([^>]*)>").unwrap());

fn attributes(source: &str) -> HashMap<String, String> {
    ATTR_RE
        .captures_iter(source)
        .map(|c| {
            let value = c
                .get(3)
                .or(c.get(4))
                .or(c.get(5))
                .map(|m| m.as_str())
                .unwrap_or("");
            (c[1].to_ascii_lowercase(), decode_entities(value))
        })
        .collect()
}

pub fn is_relay_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    parsed.scheme() == "https"
        && is_site_host(&host)
        && host.starts_with('t')
        && host
            .split('.')
            .next()
            .is_some_and(|label| label[1..].chars().all(|c| c.is_ascii_digit()))
        && parsed.path().starts_with("/f/")
}

fn expiry_of(url: &Url) -> Option<i64> {
    url.query_pairs()
        .find(|(k, _)| k == "e")
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .filter(|v| *v > 0)
        .map(|v| v * 1000)
}

#[derive(Debug, Default, Clone)]
pub struct RelayIndex {
    /// Keyed by canonical storage path (`/aa/bb/<hash>.<ext>`).
    pub by_path: HashMap<String, RelayLink>,
    /// Keyed by the file name the page offers.
    pub by_name: HashMap<String, RelayLink>,
}

pub fn parse_relay_links(html: &str) -> RelayIndex {
    let mut index = RelayIndex::default();
    for caps in ANCHOR_RE.captures_iter(html) {
        let attrs = attributes(&caps[1]);
        let Some(href) = attrs.get("href").filter(|h| is_relay_url(h)) else {
            continue;
        };
        let Ok(url) = Url::parse(href) else { continue };
        let name = attrs
            .get("download")
            .map(|d| d.trim().to_string())
            .filter(|d| !d.is_empty())
            .or_else(|| {
                url.path_segments()
                    .and_then(|mut s| s.next_back())
                    .map(|s| percent_decode_str(s).decode_utf8_lossy().into_owned())
                    .filter(|s| !s.is_empty())
            });
        let link = RelayLink {
            url: href.clone(),
            expires_at: expiry_of(&url),
            name: name.clone(),
        };
        if let Some(img) = IMG_RE.captures(&caps[2]) {
            let img_attrs = attributes(&img[1]);
            let src = img_attrs
                .get("data-src")
                .filter(|s| !s.is_empty())
                .or(img_attrs.get("src"));
            if let Some(src) = src {
                let base = Url::parse("https://img.pawchive.pw").unwrap();
                if let Ok(abs) = base.join(src) {
                    let path = abs.path().trim_start_matches("/thumbnail/data").to_string();
                    if let Some(canonical) = canonical_path(&path) {
                        index.by_path.insert(canonical, link.clone());
                    }
                }
            }
        }
        if let Some(name) = name {
            index.by_name.entry(name).or_insert(link);
        }
    }
    index
}

/// The relay link for a file, if the page offered one that has not expired.
pub fn find_relay(index: &RelayIndex, path: &str, name: &str, now_ms: i64) -> Option<RelayLink> {
    let link = canonical_path(path)
        .and_then(|c| index.by_path.get(&c))
        .or_else(|| index.by_name.get(name))?;
    // Leave a minute of slack so a download does not start on a dying link.
    if link.expires_at.is_some_and(|at| at - 60_000 <= now_ms) {
        return None;
    }
    Some(link.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_relay_links_by_thumbnail_and_name() {
        let h = "ab".repeat(32);
        let html = format!(
            r#"<a href="https://t4.pawchive.pw/f/1/clip%20one.mp4?e=2000000000&amp;s=sig" download="clip one.mp4">
                 <img data-src="/thumbnail/data/ab/ab/{h}.jpg"></a>
               <a href="https://t2.pawchive.pw/f/2/other.zip?e=1&s=x">x</a>
               <a href="https://n1.pawchive.pw/data/x.png">not relay</a>"#
        );
        let index = parse_relay_links(&html);
        assert_eq!(index.by_path.len(), 1);
        let link = find_relay(&index, &format!("/2ab/ab/{h}.jpg"), "nope", 0).unwrap();
        assert_eq!(
            link.url,
            "https://t4.pawchive.pw/f/1/clip%20one.mp4?e=2000000000&s=sig"
        );
        assert_eq!(link.expires_at, Some(2_000_000_000_000));
        assert_eq!(link.name.as_deref(), Some("clip one.mp4"));
        // Expired links are ignored.
        assert!(find_relay(&index, "/x", "other.zip", 10_000_000).is_none());
        assert!(index.by_name.contains_key("other.zip"));
    }

    #[test]
    fn recognises_relay_hosts() {
        assert!(is_relay_url("https://t4.pawchive.pw/f/1/a"));
        assert!(is_relay_url("https://t.pawchive.pw/f/1/a"));
        assert!(!is_relay_url("http://t4.pawchive.pw/f/1/a"));
        assert!(!is_relay_url("https://n4.pawchive.pw/f/1/a"));
        assert!(!is_relay_url("https://t4.pawchive.pw/data/1"));
        assert!(!is_relay_url("https://t4.evil.com/f/1"));
    }
}
