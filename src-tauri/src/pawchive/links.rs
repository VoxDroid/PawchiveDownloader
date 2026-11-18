//! Links to outside file hosts (Mega, Google Drive, Dropbox and the like) and
//! archive passwords found in a post's text. Creators often share the large
//! files there instead of attaching them.

use std::sync::LazyLock;

use regex::Regex;
use url::Url;

use super::normalize::html_to_text;

/// Hosts that serve downloads. Matched against the end of the host name.
const FILE_HOSTS: [&str; 46] = [
    "mega.nz",
    "mega.co.nz",
    "mega.io",
    "drive.google.com",
    "docs.google.com",
    "dropbox.com",
    "db.tt",
    "mediafire.com",
    "gofile.io",
    "pixeldrain.com",
    "catbox.moe",
    "workupload.com",
    "1drv.ms",
    "onedrive.live.com",
    "sharepoint.com",
    "box.com",
    "terabox.com",
    "1024terabox.com",
    "krakenfiles.com",
    "files.fm",
    "wetransfer.com",
    "we.tl",
    "send.cm",
    "buzzheavier.com",
    "cyberdrop.me",
    "uploadhaven.com",
    "4shared.com",
    "yadi.sk",
    "disk.yandex.com",
    "disk.yandex.ru",
    "pan.baidu.com",
    "alipan.com",
    "aliyundrive.com",
    "icloud.com",
    "drive.proton.me",
    "swisstransfer.com",
    "filen.io",
    "gigafile.nu",
    "firestorage.jp",
    "xgf.nu",
    "getuploader.com",
    "axfc.net",
    "fileditchfiles.me",
    "qiwi.gg",
    "ranoz.gg",
    "bowfile.com",
];

/// Domains with numbered or changing top-level names (bunkr.si, bunkr.cr...).
const FILE_HOST_NAMES: [&str; 2] = ["bunkr", "bunkrr"];

static HREF_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)href\s*=\s*["']([^"']+)["']"#).unwrap());
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?i)\bhttps?://[^\s<>"'\]\)]+"#).unwrap());
/// "Password: abc", "pass：abc", "PW=abc", "パスワード：abc", "密码：abc". A
/// separator is required, so ordinary text such as "pass the test" is not
/// taken for a password.
static PASSWORD_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?:\b(?:password|passwd|pass|pwd|pw|contraseña|senha|passwort|mot de passe|пароль)\b|パスワード|解凍パス|パス|密码|解压码|提取码|비밀번호|비번)\s*[:：=]\s*[「『"']?([^\s「」『』"'<>]{2,64})"#,
    )
    .unwrap()
});

fn is_file_host(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    if FILE_HOSTS
        .iter()
        .any(|h| host == *h || host.ends_with(&format!(".{h}")))
    {
        // Google Docs pages are documents, not downloads, unless shared files.
        return host != "docs.google.com" || url.path().contains("/file/");
    }
    host.split('.')
        .rev()
        .nth(1)
        .is_some_and(|name| FILE_HOST_NAMES.contains(&name))
}

fn clean(raw: &str) -> Option<Url> {
    let raw = raw
        .trim()
        .replace("&amp;", "&")
        .trim_end_matches(['.', ',', ';', ':', '!', '?', '>'])
        .to_string();
    let url = Url::parse(&raw).ok()?;
    matches!(url.scheme(), "http" | "https").then_some(url)
}

/// File-host links in a post's HTML, in order, without duplicates.
pub fn file_host_links(content_html: &str) -> Vec<String> {
    let text = html_to_text(content_html);
    let candidates = HREF_RE
        .captures_iter(content_html)
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .chain(URL_RE.find_iter(&text).map(|m| m.as_str().to_string()));
    let mut out: Vec<String> = Vec::new();
    for raw in candidates {
        let Some(url) = clean(&raw) else { continue };
        if !is_file_host(&url) {
            continue;
        }
        let s = url.to_string();
        if !out.contains(&s) {
            out.push(s);
        }
        if out.len() >= 200 {
            break;
        }
    }
    out
}

/// Passwords written next to "password", "pass", "パスワード" and the like.
pub fn passwords(content_html: &str) -> Vec<String> {
    let text = html_to_text(content_html);
    let mut out: Vec<String> = Vec::new();
    for caps in PASSWORD_RE.captures_iter(&text) {
        let value = caps[1]
            .trim_end_matches(['.', ',', ';', ')', '）'])
            .to_string();
        // A link is not a password ("Download: https://...").
        if value.len() < 2 || value.starts_with("http") || value.contains("://") {
            continue;
        }
        if !out.contains(&value) {
            out.push(value);
        }
        if out.len() >= 20 {
            break;
        }
    }
    out
}

/// The text of a `links.txt` file, or None when the post has neither.
pub fn links_text(
    title: &str,
    url: &str,
    links: &[String],
    passwords: &[String],
) -> Option<String> {
    if links.is_empty() && passwords.is_empty() {
        return None;
    }
    let mut out = format!("{title}\n{url}\n");
    if !links.is_empty() {
        out.push_str("\nLinks\n");
        for l in links {
            out.push_str(l);
            out.push('\n');
        }
    }
    if !passwords.is_empty() {
        out.push_str("\nPasswords\n");
        for p in passwords {
            out.push_str(p);
            out.push('\n');
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_file_host_links_once() {
        let html = r#"<p>Full set: <a href="https://mega.nz/folder/abc#key">mega</a></p>
            <p>Mirror https://drive.google.com/file/d/xyz/view?usp=sharing.</p>
            <p>https://mega.nz/folder/abc#key and https://www.dropbox.com/s/q/a.zip?dl=0</p>
            <p>https://bunkr.si/a/zz https://twitter.com/me https://docs.google.com/document/d/1</p>
            <p>https://docs.google.com/file/d/2</p>"#;
        assert_eq!(
            file_host_links(html),
            [
                "https://mega.nz/folder/abc#key",
                "https://drive.google.com/file/d/xyz/view?usp=sharing",
                "https://www.dropbox.com/s/q/a.zip?dl=0",
                "https://bunkr.si/a/zz",
                "https://docs.google.com/file/d/2",
            ]
        );
    }

    #[test]
    fn finds_passwords_in_many_languages() {
        let html = "<p>Password: cat123</p><p>パスワード：「にゃん」</p><p>密码：abcd</p>\
                    <p>pass: https://mega.nz/x</p><p>pw: cat123</p><p>compass: north</p>\
                    <p>pass the test</p>";
        assert_eq!(passwords(html), ["cat123", "にゃん", "abcd"]);
        assert!(passwords("<p>Nothing here</p>").is_empty());
    }

    #[test]
    fn writes_a_links_file_only_when_there_is_something() {
        assert!(links_text("T", "u", &[], &[]).is_none());
        let text = links_text("T", "u", &["https://mega.nz/a".into()], &["pw".into()]).unwrap();
        assert_eq!(text, "T\nu\n\nLinks\nhttps://mega.nz/a\n\nPasswords\npw\n");
    }
}
