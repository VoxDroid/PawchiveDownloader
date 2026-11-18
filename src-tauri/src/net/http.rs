//! The HTTP client. Pawchive's file servers reject browser user agents coming
//! from programs that are not browsers, and the site asks automated clients to
//! identify themselves, so every request carries the app's own user agent.

use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const CONTACT: &str = "izeno.contact@gmail.com";

pub fn user_agent() -> String {
    format!(
        "PawchiveDownloader/{} (+mailto:{CONTACT})",
        env!("CARGO_PKG_VERSION")
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProxyMode {
    /// Use the system's proxy settings (environment variables).
    #[default]
    System,
    /// Connect directly, ignoring any system proxy.
    None,
    /// The proxy URL in the settings (http://, https://, socks5://, socks5h://).
    Custom,
    /// Tor's SOCKS port on this computer (socks5h://127.0.0.1:9050).
    Tor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxySettings {
    pub mode: ProxyMode,
    pub url: String,
}

pub const TOR_PROXY: &str = "socks5h://127.0.0.1:9050";

/// Checks a proxy URL; returns a short reason when it is not usable.
pub fn validate_proxy_url(url: &str) -> Result<(), String> {
    let parsed = url::Url::parse(url.trim()).map_err(|_| "invalid".to_string())?;
    match parsed.scheme() {
        "http" | "https" | "socks5" | "socks5h" => {}
        _ => return Err("scheme".into()),
    }
    if parsed.host_str().unwrap_or("").is_empty() {
        return Err("host".into());
    }
    Ok(())
}

pub fn build_client(proxy: &ProxySettings) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .user_agent(user_agent())
        .connect_timeout(Duration::from_secs(20))
        .read_timeout(Duration::from_secs(90))
        .pool_idle_timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(8))
        .https_only(false);
    match proxy.mode {
        ProxyMode::System => {}
        ProxyMode::None => builder = builder.no_proxy(),
        ProxyMode::Tor => {
            builder = builder.proxy(reqwest::Proxy::all(TOR_PROXY).map_err(|e| e.to_string())?);
        }
        ProxyMode::Custom => {
            validate_proxy_url(&proxy.url)?;
            builder =
                builder.proxy(reqwest::Proxy::all(proxy.url.trim()).map_err(|e| e.to_string())?);
        }
    }
    builder.build().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifies_itself() {
        assert!(user_agent().starts_with("PawchiveDownloader/"));
        assert!(user_agent().contains("mailto:"));
    }

    #[test]
    fn validates_proxies() {
        assert!(validate_proxy_url("socks5h://127.0.0.1:9050").is_ok());
        assert!(validate_proxy_url("http://proxy:8080").is_ok());
        assert_eq!(validate_proxy_url("ftp://x"), Err("scheme".into()));
        assert_eq!(validate_proxy_url("nope"), Err("invalid".into()));
        for mode in [ProxyMode::System, ProxyMode::None, ProxyMode::Tor] {
            assert!(build_client(&ProxySettings {
                mode,
                url: String::new()
            })
            .is_ok());
        }
        assert!(build_client(&ProxySettings {
            mode: ProxyMode::Custom,
            url: "bad".into()
        })
        .is_err());
    }
}
