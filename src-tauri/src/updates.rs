//! Checks GitHub for a newer release. The app only tells the user and links
//! to the release page; it never downloads or installs anything by itself.

use serde::{Deserialize, Serialize};

pub const RELEASES_API: &str =
    "https://api.github.com/repos/VoxDroid/PawchiveDownloader/releases/latest";
pub const RELEASES_PAGE: &str = "https://github.com/VoxDroid/PawchiveDownloader/releases";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub url: String,
    pub notes: Option<String>,
    pub published_at: Option<String>,
    /// Set when the check itself failed (offline, no release yet...).
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    html_url: Option<String>,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

fn parse_version(tag: &str) -> Option<semver::Version> {
    semver::Version::parse(tag.trim().trim_start_matches(['v', 'V'])).ok()
}

fn base(current: &str) -> UpdateInfo {
    UpdateInfo {
        current: current.to_string(),
        latest: None,
        available: false,
        url: RELEASES_PAGE.to_string(),
        notes: None,
        published_at: None,
        error: None,
    }
}

/// Compares a release answer with the running version.
pub fn evaluate(current: &str, body: &str) -> UpdateInfo {
    let mut info = base(current);
    match serde_json::from_str::<Release>(body) {
        Ok(r) if !r.draft && !r.prerelease => {
            let latest = parse_version(&r.tag_name);
            info.available =
                matches!((&latest, parse_version(current)), (Some(l), Some(c)) if *l > c);
            info.latest = latest.map(|v| v.to_string()).or(Some(r.tag_name));
            info.url = r.html_url.unwrap_or(info.url);
            info.notes = r.body.map(|b| b.chars().take(4000).collect());
            info.published_at = r.published_at;
        }
        Ok(_) => info.error = Some("no-release".into()),
        Err(_) => info.error = Some("unexpected".into()),
    }
    info
}

pub async fn check(http: &reqwest::Client) -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION");
    let response = http
        .get(RELEASES_API)
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
        Ok(r) if r.status().as_u16() == 404 => failed("no-release".into()),
        Ok(r) => failed(format!("HTTP {}", r.status().as_u16())),
        Err(e) => failed(crate::pawchive::client::describe(&e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions() {
        let newer = evaluate(
            "1.0.0",
            r#"{"tag_name":"v1.2.0","html_url":"https://x/r","body":"Notes"}"#,
        );
        assert!(newer.available);
        assert_eq!(newer.latest.as_deref(), Some("1.2.0"));
        assert_eq!(newer.url, "https://x/r");
        let same = evaluate("1.2.0", r#"{"tag_name":"1.2.0"}"#);
        assert!(!same.available);
        let pre = evaluate("1.0.0", r#"{"tag_name":"v2.0.0","prerelease":true}"#);
        assert!(!pre.available);
        assert_eq!(pre.error.as_deref(), Some("no-release"));
        assert_eq!(
            evaluate("1.0.0", "nope").error.as_deref(),
            Some("unexpected")
        );
    }
}
