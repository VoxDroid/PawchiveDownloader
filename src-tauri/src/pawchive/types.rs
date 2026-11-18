//! Shapes of the Pawchive JSON API (a Kemono fork) as observed on pawchive.pw,
//! and the normalized models the rest of the app works with.

use serde::{Deserialize, Deserializer, Serialize};

/// Accepts a string or a number (the API is not consistent about ids).
fn string_or_number<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = serde_json::Value::deserialize(d)?;
    Ok(match value {
        serde_json::Value::String(s) => s,
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    })
}

/// A file entry as the API returns it. An empty object when a post has no file.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct RawFile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
    /// Media server that holds the full file, e.g. `https://n1.pawchive.pw`.
    #[serde(default)]
    pub server: Option<String>,
    /// The site only keeps the thumbnail; the full file is not on its servers.
    #[serde(default)]
    pub preview_only: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawPost {
    #[serde(deserialize_with = "string_or_number")]
    pub id: String,
    #[serde(deserialize_with = "string_or_number")]
    pub user: String,
    pub service: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    /// Listing endpoints send a short HTML excerpt instead of `content`.
    #[serde(default)]
    pub substring: Option<String>,
    #[serde(default)]
    pub added: Option<String>,
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub edited: Option<String>,
    #[serde(default)]
    pub file: Option<RawFile>,
    #[serde(default)]
    pub attachments: Option<Vec<RawFile>>,
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
    #[serde(default)]
    pub next: Option<serde_json::Value>,
    #[serde(default)]
    pub prev: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawProfile {
    #[serde(deserialize_with = "string_or_number")]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    pub service: String,
    #[serde(default)]
    pub public_id: Option<String>,
    #[serde(default)]
    pub updated: Option<serde_json::Value>,
    #[serde(default)]
    pub post_count: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileCategory {
    Image,
    Video,
    Audio,
    Archive,
    Document,
    Text,
    Other,
}

impl FileCategory {
    pub const ALL: [FileCategory; 7] = [
        FileCategory::Image,
        FileCategory::Video,
        FileCategory::Audio,
        FileCategory::Archive,
        FileCategory::Document,
        FileCategory::Text,
        FileCategory::Other,
    ];
}

/// Where a file came from inside a post.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileRole {
    File,
    Attachment,
    Inline,
}

impl FileRole {
    pub fn as_str(self) -> &'static str {
        match self {
            FileRole::File => "file",
            FileRole::Attachment => "attachment",
            FileRole::Inline => "inline",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostFile {
    /// 1-based position among the post's unique files, used for numbering.
    pub index: u32,
    pub role: FileRole,
    /// Original file name as uploaded.
    pub name: String,
    /// Lower-case extension without the dot, may be empty.
    pub ext: String,
    /// Content hash taken from the storage path (sha256 hex) when available.
    pub hash: Option<String>,
    /// Server path as the API sent it.
    pub path: String,
    /// Direct download URL, or None when the site only has a preview.
    pub url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub preview_only: bool,
    pub category: FileCategory,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Post {
    pub id: String,
    pub service: String,
    pub creator_id: String,
    pub title: String,
    pub content_html: String,
    pub published: Option<String>,
    pub added: Option<String>,
    pub edited: Option<String>,
    pub tags: Vec<String>,
    pub files: Vec<PostFile>,
    pub url: String,
    pub next_id: Option<String>,
    pub prev_id: Option<String>,
    /// Listings leave out some files (marked `deferred`, without a path); the
    /// full post has them.
    #[serde(default)]
    pub incomplete: bool,
    /// Links to outside file hosts (Mega, Google Drive...) found in the text.
    #[serde(default)]
    pub links: Vec<String>,
    /// Archive passwords found in the text.
    #[serde(default)]
    pub passwords: Vec<String>,
}

/// A comment under a post.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    #[serde(deserialize_with = "string_or_number")]
    pub id: String,
    #[serde(default, alias = "parent_id")]
    pub parent_id: Option<serde_json::Value>,
    #[serde(default, alias = "commenter_name")]
    pub commenter_name: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub published: Option<String>,
}

/// A creator's announcement (community post).
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Announcement {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub added: Option<String>,
    #[serde(default)]
    pub published: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Creator {
    pub id: String,
    pub service: String,
    pub name: String,
    pub public_id: Option<String>,
    pub url: String,
    pub icon_url: String,
    pub banner_url: String,
    pub updated: Option<String>,
    pub post_count: Option<u64>,
}

/// A temporary, signed link to the full version of a preview-only file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayLink {
    pub url: String,
    /// Unix epoch milliseconds after which the link stops working, if known.
    pub expires_at: Option<i64>,
    /// File name the page offers for the download.
    pub name: Option<String>,
}
