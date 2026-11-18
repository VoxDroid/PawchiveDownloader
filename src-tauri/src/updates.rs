//! Checks for a newer version. The source code is free to build; official
//! ready-made builds are offered on Ko-fi. The app reads the version tags of
//! the source repository and points to Ko-fi (and to the source for people
//! who build it themselves). It never downloads or installs anything.

use serde::{Deserialize, Serialize};

pub const TAGS_API: &str =
    "https://api.github.com/repos/VoxDroid/PawchiveDownloader/tags?per_page=50";
pub const SOURCE_PAGE: &str = "https://github.com/VoxDroid/PawchiveDownloader";
pub const KOFI_PAGE: &str = "https://ko-fi.com/izeno";

/// "official" for builds from the release repository, "source" otherwise.
pub fn channel() -> &'static str {
    match option_env!("PAWCHIVE_BUILD_CHANNEL") {
        Some("official") => "official",
        _ => "source",
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    /// Where to get the new version (Ko-fi).
    pub url: String,
    /// The source code, for people who build the app themselves.
    pub source_url: String,
    /// Set when the check itself failed (offline, no version yet...).
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Tag {
    name: String,
}

fn parse_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim().trim_start_matches(['v', 'V'])).ok()
}

fn base(current: &str) -> UpdateInfo {
    UpdateInfo {
        current: current.to_string(),
        latest: None,
        available: false,
        url: KOFI_PAGE.to_string(),
        source_url: SOURCE_PAGE.to_string(),
        error: None,
    }
}

/// Picks the newest stable version from a tag listing and compares it.
pub fn evaluate(current: &str, body: &str) -> UpdateInfo {
    let mut info = base(current);
    match serde_json::from_str::<Vec<Tag>>(body) {
        Ok(tags) => {
            let latest = tags
                .iter()
                .filter_map(|t| parse_version(&t.name))
                .filter(|v| v.pre.is_empty())
                .max();
            match latest {
                Some(latest) => {
                    info.available = parse_version(current).is_some_and(|c| latest > c);
                    info.latest = Some(latest.to_string());
                }
                None => info.error = Some("no-release".into()),
            }
        }
        Err(_) => info.error = Some("unexpected".into()),
    }
    info
}

pub async fn check(http: &reqwest::Client) -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION");
    let response = http
        .get(TAGS_API)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .send()
        .await;
    let failed = |error: String| UpdateInfo {
        error: Some(error),
        ..base(current)
    };
    match response {
        Ok(r) if r.status().is_success() => match r.text().await {
            Ok(body) => evaluate(current, &body),
            Err(e) => failed(e.to_string()),
        },
        // A private or missing repository answers 404.
        Ok(r) if r.status().as_u16() == 404 => failed("no-release".into()),
        Ok(r) => failed(format!("HTTP {}", r.status().as_u16())),
        Err(e) => failed(crate::pawchive::client::describe(&e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_newest_stable_tag() {
        let tags =
            r#"[{"name":"v1.2.0"},{"name":"v1.10.0"},{"name":"v2.0.0-beta.1"},{"name":"notes"}]"#;
        let newer = evaluate("1.2.0", tags);
        assert!(newer.available);
        assert_eq!(newer.latest.as_deref(), Some("1.10.0"));
        assert_eq!(newer.url, KOFI_PAGE);
        assert_eq!(newer.source_url, SOURCE_PAGE);
        assert!(!evaluate("1.10.0", tags).available);
        assert_eq!(evaluate("1.0.0", "[]").error.as_deref(), Some("no-release"));
        assert_eq!(
            evaluate("1.0.0", "nope").error.as_deref(),
            Some("unexpected")
        );
        assert_eq!(channel(), "source");
    }
}
