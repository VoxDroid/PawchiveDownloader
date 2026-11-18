//! Post and file filters: keywords a post must have, words that skip a post
//! or a file, file types, and a limit on how many posts a scan takes.
//!
//! Terms match case-insensitively as plain text. A term written as `/.../`
//! is a regular expression instead (also case-insensitive).

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::pawchive::normalize::html_to_text;
use crate::pawchive::types::{Post, PostFile};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterScope {
    /// The title and the tags.
    #[default]
    Title,
    /// The title, the tags and the post text.
    All,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PostFilter {
    /// A post needs at least one of these (none: every post).
    pub keywords: Vec<String>,
    /// A post with any of these is skipped.
    pub skip_words: Vec<String>,
    pub scope: FilterScope,
    /// A file whose name has any of these is skipped.
    pub file_skip_words: Vec<String>,
    /// Only files with these extensions (none: every extension).
    pub extensions: Vec<String>,
    /// Files with these extensions are skipped.
    pub skip_extensions: Vec<String>,
    /// Stop a creator scan after this many posts (0: no limit).
    pub max_posts: u32,
}

pub const MAX_TERMS: usize = 100;
const MAX_TERM_LENGTH: usize = 200;

/// Cleans a list of terms typed by the user: trimmed, deduplicated, bounded,
/// and invalid regular expressions dropped.
pub fn clean_terms(terms: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for term in terms {
        let term: String = term.trim().chars().take(MAX_TERM_LENGTH).collect();
        if term.is_empty() || out.iter().any(|t| t.to_lowercase() == term.to_lowercase()) {
            continue;
        }
        if let Some(pattern) = regex_body(&term) {
            if build_regex(pattern).is_none() {
                continue;
            }
        }
        out.push(term);
        if out.len() >= MAX_TERMS {
            break;
        }
    }
    out
}

/// Extensions without dots, lower case.
pub fn clean_extensions(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for ext in list {
        let ext = ext.trim().trim_start_matches('.').to_lowercase();
        if ext.is_empty() || ext.len() > 16 || out.contains(&ext) {
            continue;
        }
        out.push(ext);
        if out.len() >= MAX_TERMS {
            break;
        }
    }
    out
}

impl PostFilter {
    pub fn cleaned(&self) -> Self {
        Self {
            keywords: clean_terms(&self.keywords),
            skip_words: clean_terms(&self.skip_words),
            scope: self.scope,
            file_skip_words: clean_terms(&self.file_skip_words),
            extensions: clean_extensions(&self.extensions),
            skip_extensions: clean_extensions(&self.skip_extensions),
            max_posts: self.max_posts.min(1_000_000),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.keywords.is_empty()
            && self.skip_words.is_empty()
            && self.file_skip_words.is_empty()
            && self.extensions.is_empty()
            && self.skip_extensions.is_empty()
            && self.max_posts == 0
    }

    pub fn matcher(&self) -> Matcher {
        Matcher {
            keywords: compile(&self.keywords),
            skip_words: compile(&self.skip_words),
            scope: self.scope,
            file_skip_words: compile(&self.file_skip_words),
            extensions: self.extensions.clone(),
            skip_extensions: self.skip_extensions.clone(),
        }
    }
}

fn regex_body(term: &str) -> Option<&str> {
    (term.len() > 2 && term.starts_with('/') && term.ends_with('/'))
        .then(|| &term[1..term.len() - 1])
}

fn build_regex(pattern: &str) -> Option<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(true)
        .size_limit(1 << 20)
        .build()
        .ok()
}

enum Term {
    Text(String),
    Pattern(Regex),
}

impl Term {
    fn found_in(&self, haystack_lower: &str, haystack: &str) -> bool {
        match self {
            Term::Text(t) => haystack_lower.contains(t.as_str()),
            Term::Pattern(r) => r.is_match(haystack),
        }
    }
}

fn compile(terms: &[String]) -> Vec<Term> {
    terms
        .iter()
        .filter_map(|t| match regex_body(t) {
            Some(p) => build_regex(p).map(Term::Pattern),
            None => Some(Term::Text(t.to_lowercase())),
        })
        .collect()
}

/// A compiled filter.
pub struct Matcher {
    keywords: Vec<Term>,
    skip_words: Vec<Term>,
    scope: FilterScope,
    file_skip_words: Vec<Term>,
    extensions: Vec<String>,
    skip_extensions: Vec<String>,
}

impl Matcher {
    /// Whether a post passes the keyword and skip-word rules.
    pub fn accepts_post(&self, post: &Post) -> bool {
        if self.keywords.is_empty() && self.skip_words.is_empty() {
            return true;
        }
        let mut text = format!("{}\n{}", post.title, post.tags.join("\n"));
        if self.scope == FilterScope::All {
            text.push('\n');
            text.push_str(&html_to_text(&post.content_html));
        }
        let lower = text.to_lowercase();
        if self.skip_words.iter().any(|t| t.found_in(&lower, &text)) {
            return false;
        }
        self.keywords.is_empty() || self.keywords.iter().any(|t| t.found_in(&lower, &text))
    }

    /// Whether a file passes the extension and file-name rules.
    pub fn accepts_file(&self, file: &PostFile) -> bool {
        let ext = file.ext.to_lowercase();
        if !self.extensions.is_empty() && !self.extensions.contains(&ext) {
            return false;
        }
        if self.skip_extensions.contains(&ext) {
            return false;
        }
        let lower = file.name.to_lowercase();
        !self
            .file_skip_words
            .iter()
            .any(|t| t.found_in(&lower, &file.name))
    }
}

/// Removes the given words from a name (case-insensitive), then tidies the
/// spaces and separators left behind.
pub fn remove_words(name: &str, words: &[String]) -> String {
    if words.is_empty() {
        return name.to_string();
    }
    let mut out = name.to_string();
    for word in words {
        let pattern = match regex_body(word) {
            Some(p) => p.to_string(),
            None => regex::escape(word),
        };
        if let Some(re) = build_regex(&pattern) {
            out = re.replace_all(&out, "").into_owned();
        }
    }
    static SPACES: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"\s{2,}").unwrap());
    static EMPTY: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"[\[\(\{]\s*[\]\)\}]").unwrap());
    let out = EMPTY.replace_all(&out, "");
    let out = SPACES.replace_all(&out, " ");
    out.trim_matches(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawchive::types::{FileCategory, FileRole};

    fn post(title: &str, tags: &[&str], content: &str) -> Post {
        Post {
            id: "1".into(),
            service: "fanbox".into(),
            creator_id: "2".into(),
            title: title.into(),
            content_html: content.into(),
            published: None,
            added: None,
            edited: None,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            files: vec![],
            url: String::new(),
            next_id: None,
            prev_id: None,
            incomplete: false,
        }
    }

    fn file(name: &str) -> PostFile {
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_lowercase())
            .unwrap_or_default();
        PostFile {
            index: 1,
            role: FileRole::Attachment,
            name: name.into(),
            ext,
            hash: None,
            path: String::new(),
            url: None,
            thumbnail_url: None,
            preview_only: false,
            category: FileCategory::Other,
        }
    }

    fn terms(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn keywords_and_skip_words() {
        let f = PostFilter {
            keywords: terms(&["Comic", "/ch(apter)?\\s*\\d+/"]),
            skip_words: terms(&["WIP"]),
            ..Default::default()
        }
        .cleaned();
        let m = f.matcher();
        assert!(m.accepts_post(&post("My comic page", &[], "")));
        assert!(m.accepts_post(&post("Chapter 12", &[], "")));
        assert!(m.accepts_post(&post("x", &["comic"], "")));
        assert!(!m.accepts_post(&post("Comic wip", &[], "")));
        assert!(!m.accepts_post(&post("Sketch", &[], "<p>comic</p>")));
        let all = PostFilter {
            scope: FilterScope::All,
            ..f
        }
        .matcher();
        assert!(all.accepts_post(&post("Sketch", &[], "<p>comic</p>")));
        assert!(PostFilter::default()
            .matcher()
            .accepts_post(&post("", &[], "")));
    }

    #[test]
    fn files_by_extension_and_name() {
        let f = PostFilter {
            extensions: terms(&[".PNG", "jpg", "png"]),
            file_skip_words: terms(&["preview"]),
            ..Default::default()
        }
        .cleaned();
        assert_eq!(f.extensions, ["png", "jpg"]);
        let m = f.matcher();
        assert!(m.accepts_file(&file("a.PNG")));
        assert!(!m.accepts_file(&file("a.psd")));
        assert!(!m.accepts_file(&file("Preview_1.png")));
        let skip = PostFilter {
            skip_extensions: terms(&["psd"]),
            ..Default::default()
        }
        .cleaned()
        .matcher();
        assert!(skip.accepts_file(&file("a.png")));
        assert!(!skip.accepts_file(&file("a.PSD")));
    }

    #[test]
    fn cleans_terms() {
        let t = clean_terms(&terms(&[" a ", "A", "", "/(/", "/ok/"]));
        assert_eq!(t, ["a", "/ok/"]);
        assert!(PostFilter::default().is_empty());
    }

    #[test]
    fn removes_words_from_names() {
        let words = terms(&["[HD]", "patreon", "/v\\d+/"]);
        assert_eq!(remove_words("Cat [HD] - Patreon v2", &words), "Cat");
        assert_eq!(remove_words("(patreon) Dog_", &words), "Dog");
        assert_eq!(remove_words("keep", &[]), "keep");
    }
}
