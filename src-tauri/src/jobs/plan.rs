//! Turns posts into download items.

use std::collections::HashSet;

use crate::naming::template::{
    build_archive_path, build_file_path, build_folder_path, NameContext,
};
use crate::pawchive::links::links_text;
use crate::pawchive::normalize::html_to_text;
use crate::pawchive::types::{FileCategory, Post, PostFile};
use crate::settings::{ContentFormat, DownloadMode, PreviewOnlyAction};

use super::filter::remove_words;
use super::model::*;

/// The naming context with the user's unwanted words taken out of the title
/// and the file name (the extension stays).
fn cleaned_context(
    post: &Post,
    creator: Option<&CreatorInfo>,
    file: Option<&PostFile>,
    options: &JobOptions,
    now_ms: i64,
) -> NameContext {
    let mut ctx = name_context(post, creator, file, now_ms);
    if options.name_remove.is_empty() {
        return ctx;
    }
    ctx.post_title = remove_words(&ctx.post_title, &options.name_remove);
    if !ctx.file_name.is_empty() {
        let (stem, ext) = match ctx.file_name.rfind('.') {
            Some(dot) if dot > 0 => ctx.file_name.split_at(dot),
            _ => (ctx.file_name.as_str(), ""),
        };
        let stem = remove_words(stem, &options.name_remove);
        let stem = if stem.is_empty() {
            "file".to_string()
        } else {
            stem
        };
        ctx.file_name = format!("{stem}{ext}");
    }
    ctx
}

fn name_context(
    post: &Post,
    creator: Option<&CreatorInfo>,
    file: Option<&PostFile>,
    now_ms: i64,
) -> NameContext {
    NameContext {
        service: post.service.clone(),
        creator_id: post.creator_id.clone(),
        creator_name: creator
            .map(|c| c.name.clone())
            .unwrap_or_else(|| post.creator_id.clone()),
        post_id: post.id.clone(),
        post_title: post.title.clone(),
        published: post.published.clone(),
        added: post.added.clone(),
        index: file.map(|f| f.index).unwrap_or(0),
        count: post.files.len() as u32,
        file_name: file.map(|f| f.name.clone()).unwrap_or_default(),
        ext: file.map(|f| f.ext.clone()).unwrap_or_default(),
        hash: file.and_then(|f| f.hash.clone()),
        role: file
            .map(|f| f.role.as_str().to_string())
            .unwrap_or_default(),
        now_ms: Some(now_ms),
    }
}

/// `name.png` becomes `name_preview.png`, as KemonoDownloader names previews.
fn preview_file(file: &PostFile) -> PostFile {
    let stem = match file.name.rfind('.') {
        Some(dot) if dot > 0 => &file.name[..dot],
        _ => &file.name,
    };
    let ext = if file.ext.is_empty() {
        "jpg".to_string()
    } else {
        file.ext.clone()
    };
    PostFile {
        name: format!("{stem}_preview.{ext}"),
        ext,
        ..file.clone()
    }
}

pub fn metadata_json(post: &Post, creator: Option<&CreatorInfo>) -> String {
    let files: Vec<_> = post
        .files
        .iter()
        .map(|f| {
            serde_json::json!({
                "index": f.index, "name": f.name, "role": f.role, "hash": f.hash,
                "url": f.url, "previewOnly": f.preview_only,
            })
        })
        .collect();
    let value = serde_json::json!({
        "id": post.id,
        "service": post.service,
        "creator": { "id": post.creator_id, "name": creator.map(|c| c.name.clone()) },
        "title": post.title,
        "published": post.published,
        "added": post.added,
        "edited": post.edited,
        "tags": post.tags,
        "url": post.url,
        "files": files,
    });
    format!("{}\n", serde_json::to_string_pretty(&value).unwrap())
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn content_html_document(post: &Post) -> String {
    let title = escape_html(&post.title);
    format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{title}</title></head>\n<body>\n<h1>{title}</h1>\n<p><a href=\"{url}\">{url}</a></p>\n{body}\n</body></html>\n",
        url = escape_html(&post.url),
        body = post.content_html
    )
}

fn content_text(post: &Post) -> String {
    let mut header = vec![post.title.clone()];
    if let Some(p) = &post.published {
        header.push(format!("Published: {p}"));
    }
    header.push(post.url.clone());
    let header: Vec<_> = header.into_iter().filter(|s| !s.is_empty()).collect();
    format!(
        "{}\n\n{}\n",
        header.join("\n"),
        html_to_text(&post.content_html)
    )
}

struct Planned {
    file: PostFile,
    path: String,
    entry_name: String,
    url: Option<String>,
    relay: bool,
    skip: Option<SkipReason>,
    fallback: bool,
}

fn plan_file(
    post: &Post,
    creator: Option<&CreatorInfo>,
    original: &PostFile,
    options: &JobOptions,
    known: &HashSet<String>,
    now_ms: i64,
) -> Planned {
    let mut file = original.clone();
    let mut url = file.url.clone();
    let mut relay = false;
    let mut skip = None;
    let mut fallback = false;

    if options.mode == DownloadMode::Thumbnails {
        url = file.thumbnail_url.clone();
        file = preview_file(&file);
        fallback = true;
    } else if file.preview_only {
        match options.preview_only {
            PreviewOnlyAction::Relay => relay = true,
            PreviewOnlyAction::Thumbnail if file.thumbnail_url.is_some() => {
                url = file.thumbnail_url.clone();
                file = preview_file(&file);
                fallback = true;
            }
            _ => skip = Some(SkipReason::PreviewOnly),
        }
    }
    if skip.is_none() && options.skip_downloaded && !fallback {
        if let Some(hash) = &file.hash {
            if known.contains(hash) {
                skip = Some(SkipReason::Duplicate);
            }
        }
    }
    let ctx = cleaned_context(post, creator, Some(&file), options, now_ms);
    let path = build_file_path(&options.folder_template, &options.file_template, &ctx);
    let entry_name = build_file_path("", &options.file_template, &ctx);
    Planned {
        file,
        path,
        entry_name,
        url,
        relay,
        skip,
        fallback,
    }
}

fn base_item(post: &Post, id: String, label: String, path: String, kind: ItemKind) -> Item {
    Item {
        id,
        post: PostRef {
            service: post.service.clone(),
            creator_id: post.creator_id.clone(),
            post_id: post.id.clone(),
        },
        label,
        path,
        target: None,
        status: ItemStatus::Pending,
        attempts: 0,
        bytes: 0,
        total_bytes: 0,
        error: None,
        skip_reason: None,
        not_before: None,
        fallback: false,
        waits: 0,
        failed_at: None,
        auto_retries: 0,
        passwords: Vec::new(),
        force: false,
        kind,
    }
}

fn file_item(post: &Post, p: &Planned) -> Item {
    let mut item = base_item(
        post,
        format!("{}:{}", post.id, p.file.index),
        p.file.name.clone(),
        p.path.clone(),
        ItemKind::File {
            source: FileSource {
                url: p.url.clone(),
                relay: p.relay,
                path: p.file.path.clone(),
                name: p.file.name.clone(),
                thumbnail: if p.fallback {
                    None
                } else {
                    p.file.thumbnail_url.clone()
                },
            },
            hash: if p.fallback {
                None
            } else {
                p.file.hash.clone()
            },
            category: p.file.category,
        },
    );
    item.fallback = p.fallback;
    if let Some(skip) = p.skip {
        item.status = ItemStatus::Skipped;
        item.skip_reason = Some(skip);
    }
    item
}

/// Turns one post into download items: a file item per file (or a single ZIP
/// item holding them), plus the post text and metadata when asked for.
///
/// `file_indexes` is an explicit user selection and wins over the category
/// filter; without it the job's categories decide.
pub fn plan_post(
    post: &Post,
    creator: Option<&CreatorInfo>,
    options: &JobOptions,
    file_indexes: Option<&[u32]>,
    known: &HashSet<String>,
    now_ms: i64,
) -> Vec<Item> {
    let chosen: Vec<&PostFile> = match file_indexes {
        Some(indexes) => post
            .files
            .iter()
            .filter(|f| indexes.contains(&f.index))
            .collect(),
        None => {
            let matcher = options.filter.matcher();
            post.files
                .iter()
                .filter(|f| options.categories.contains(&f.category) && matcher.accepts_file(f))
                .collect()
        }
    };
    let thumbnails = options.mode == DownloadMode::Thumbnails;
    let planned: Vec<Planned> = chosen
        .into_iter()
        .filter(|f| !thumbnails || f.thumbnail_url.is_some())
        .map(|f| plan_file(post, creator, f, options, known, now_ms))
        .collect();
    let folder_ctx = cleaned_context(post, creator, None, options, now_ms);
    let folder = build_folder_path(&options.folder_template, &folder_ctx);
    let in_folder = |name: &str| {
        if folder.is_empty() {
            name.to_string()
        } else {
            format!("{folder}/{name}")
        }
    };

    let mut texts: Vec<(&str, String)> = Vec::new();
    let already_saved = !planned.is_empty()
        && planned
            .iter()
            .all(|p| p.skip == Some(SkipReason::Duplicate));
    let has_content = !already_saved && !html_to_text(&post.content_html).is_empty();
    // The thumbnail mode always keeps the description, like KemonoDownloader's.
    let content = if thumbnails && options.save_content == ContentFormat::None {
        ContentFormat::Txt
    } else {
        options.save_content
    };
    if has_content && matches!(content, ContentFormat::Txt | ContentFormat::Both) {
        texts.push(("content.txt", content_text(post)));
    }
    if has_content && matches!(content, ContentFormat::Html | ContentFormat::Both) {
        texts.push(("content.html", content_html_document(post)));
    }
    if options.save_metadata {
        texts.push(("post.json", metadata_json(post, creator)));
    }
    if options.save_links && !already_saved {
        if let Some(text) = links_text(&post.title, &post.url, &post.links, &post.passwords) {
            texts.push(("links.txt", text));
        }
    }
    // When posts share a folder (no post-specific part in the template),
    // each post's text gets its id in the name so they do not collide.
    let shared_folder = !per_post_folder(&options.folder_template);
    let text_item = |name: &str, text: &String| {
        let file = if shared_folder {
            format!("{}_{name}", post.id)
        } else {
            name.to_string()
        };
        base_item(
            post,
            format!("{}:{name}", post.id),
            file.clone(),
            in_folder(&file),
            ItemKind::Text { text: text.clone() },
        )
    };

    // Comments are fetched when the item runs; next to the archive in ZIP mode.
    let comments = (options.save_comments && !already_saved).then(|| {
        let path = if options.mode == DownloadMode::Zip {
            let archive = build_archive_path(
                &options.folder_template,
                &options.archive_template,
                &folder_ctx,
            );
            format!("{} - comments.txt", archive.trim_end_matches(".zip"))
        } else if shared_folder {
            in_folder(&format!("{}_comments.txt", post.id))
        } else {
            in_folder("comments.txt")
        };
        let label = path
            .rsplit('/')
            .next()
            .unwrap_or("comments.txt")
            .to_string();
        base_item(
            post,
            format!("{}:comments", post.id),
            label,
            path,
            ItemKind::Comments {
                title: post.title.clone(),
                url: post.url.clone(),
            },
        )
    });

    // Files the post shares on Mega, Google Drive and other hosts.
    let mut outside: Vec<Item> = Vec::new();
    if options.download_links && !already_saved {
        let base = if options.mode == DownloadMode::Zip {
            let archive = build_archive_path(
                &options.folder_template,
                &options.archive_template,
                &folder_ctx,
            );
            format!("{} - links", archive.trim_end_matches(".zip"))
        } else {
            in_folder("Links")
        };
        for (n, link) in post.links.iter().enumerate() {
            let Some(host) = crate::hosts::detect(link) else {
                continue;
            };
            let mut item = base_item(
                post,
                format!("{}:link:{n}", post.id),
                format!("{}: {}", host.name(), short_link(link)),
                format!("{base}/{}", host.name()),
                ItemKind::External {
                    url: link.clone(),
                    host,
                },
            );
            item.passwords = post.passwords.clone();
            outside.push(item);
        }
    }

    if options.mode != DownloadMode::Zip {
        let mut items: Vec<Item> = planned.iter().map(|p| file_item(post, p)).collect();
        for item in items.iter_mut() {
            item.passwords = post.passwords.clone();
        }
        items.extend(texts.iter().map(|(n, t)| text_item(n, t)));
        items.extend(comments);
        items.extend(outside);
        return items;
    }

    // ZIP mode: one archive per post. Skipped files are still listed so the
    // job report can say why they are missing; videos may go alongside.
    let mut items = Vec::new();
    let mut entries = Vec::new();
    for p in &planned {
        let separate = options.zip_videos_separately && p.file.category == FileCategory::Video;
        if p.skip.is_some() || separate {
            items.push(file_item(post, p));
            continue;
        }
        entries.push(ZipEntry {
            name: p.entry_name.clone(),
            source: Some(FileSource {
                url: p.url.clone(),
                relay: p.relay,
                path: p.file.path.clone(),
                name: p.file.name.clone(),
                thumbnail: if p.fallback {
                    None
                } else {
                    p.file.thumbnail_url.clone()
                },
            }),
            text: None,
            hash: if p.fallback {
                None
            } else {
                p.file.hash.clone()
            },
            category: p.file.category,
            fetched: false,
            failed: false,
        });
    }
    if entries.iter().any(|e| e.source.is_some()) {
        for (name, text) in &texts {
            entries.push(ZipEntry {
                name: name.to_string(),
                source: None,
                text: Some(text.clone()),
                hash: None,
                category: FileCategory::Text,
                fetched: true,
                failed: false,
            });
        }
        let archive = build_archive_path(
            &options.folder_template,
            &options.archive_template,
            &folder_ctx,
        );
        let label = archive
            .rsplit('/')
            .next()
            .unwrap_or("archive.zip")
            .to_string();
        items.insert(
            0,
            base_item(
                post,
                format!("{}:zip", post.id),
                label,
                archive,
                ItemKind::Zip { entries, parts: 0 },
            ),
        );
    } else {
        // Nothing binary left to archive: save the texts as plain files instead.
        items.extend(texts.iter().map(|(n, t)| text_item(n, t)));
    }
    items.extend(comments);
    items.extend(outside);
    items
}

/// `mega.nz/folder/AbCd...` for labels.
fn short_link(link: &str) -> String {
    let rest = link
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.");
    let rest = rest.split('#').next().unwrap_or(rest);
    if rest.chars().count() > 48 {
        format!("{}...", rest.chars().take(46).collect::<String>())
    } else {
        rest.to_string()
    }
}

/// The folder a creator's own files go in: the folder template up to the
/// first part that belongs to a single post.
pub fn creator_folder(options: &JobOptions, creator: &CreatorInfo, now_ms: i64) -> String {
    let shared: Vec<&str> = options
        .folder_template
        .split('/')
        .take_while(|segment| !per_post_folder(segment))
        .collect();
    let ctx = NameContext {
        service: creator.service.clone(),
        creator_id: creator.id.clone(),
        creator_name: creator.name.clone(),
        now_ms: Some(now_ms),
        ..Default::default()
    };
    build_folder_path(&shared.join("/"), &ctx)
}

/// The item that saves a creator's announcements.
pub fn announcements_item(options: &JobOptions, creator: &CreatorInfo, now_ms: i64) -> Item {
    let folder = creator_folder(options, creator, now_ms);
    let path = if folder.is_empty() {
        "announcements.txt".to_string()
    } else {
        format!("{folder}/announcements.txt")
    };
    Item {
        id: "creator:announcements".into(),
        post: PostRef {
            service: creator.service.clone(),
            creator_id: creator.id.clone(),
            post_id: String::new(),
        },
        label: "announcements.txt".into(),
        path,
        target: None,
        status: ItemStatus::Pending,
        attempts: 0,
        bytes: 0,
        total_bytes: 0,
        error: None,
        skip_reason: None,
        not_before: None,
        fallback: false,
        waits: 0,
        failed_at: None,
        auto_retries: 0,
        passwords: Vec::new(),
        force: false,
        kind: ItemKind::Announcements {
            name: creator.name.clone(),
        },
    }
}

/// The text of a comments file, replies indented under their parent.
pub fn comments_text(
    title: &str,
    url: &str,
    comments: &[crate::pawchive::types::Comment],
) -> String {
    use std::collections::HashMap;
    let id_of = |v: &Option<serde_json::Value>| match v {
        Some(serde_json::Value::String(s)) if !s.is_empty() => Some(s.clone()),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    };
    let mut children: HashMap<String, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    let ids: std::collections::HashSet<&str> = comments.iter().map(|c| c.id.as_str()).collect();
    for (i, c) in comments.iter().enumerate() {
        match id_of(&c.parent_id).filter(|p| ids.contains(p.as_str()) && *p != c.id) {
            Some(parent) => children.entry(parent).or_default().push(i),
            None => roots.push(i),
        }
    }
    let mut out = format!("Comments on {title}\n{url}\n");
    fn write(
        out: &mut String,
        comments: &[crate::pawchive::types::Comment],
        children: &HashMap<String, Vec<usize>>,
        i: usize,
        depth: usize,
    ) {
        if depth > 20 {
            return;
        }
        let c = &comments[i];
        let pad = "    ".repeat(depth);
        let who = c.commenter_name.as_deref().unwrap_or("Anonymous");
        let when = c.published.as_deref().unwrap_or("").replace('T', " ");
        out.push_str(&format!("\n{pad}{who}  {when}\n"));
        for line in html_to_text(c.content.as_deref().unwrap_or("")).lines() {
            out.push_str(&format!("{pad}{line}\n"));
        }
        for &child in children.get(&c.id).into_iter().flatten() {
            write(out, comments, children, child, depth + 1);
        }
    }
    for i in roots {
        write(&mut out, comments, &children, i, 0);
    }
    out
}

/// The text of an announcements file, newest first.
pub fn announcements_text(name: &str, list: &[crate::pawchive::types::Announcement]) -> String {
    let mut out = format!("Announcements by {name}\n");
    for a in list {
        let when = a
            .published
            .as_deref()
            .or(a.added.as_deref())
            .unwrap_or("")
            .replace('T', " ");
        out.push_str(&format!("\n{when}\n"));
        out.push_str(html_to_text(a.content.as_deref().unwrap_or("")).trim());
        out.push_str("\n\n----\n");
    }
    out
}

/// Whether a folder template gives each post a folder of its own.
pub fn per_post_folder(template: &str) -> bool {
    static POST: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)\{(post_id|title|published|added)").unwrap()
    });
    POST.is_match(template)
}

/// Hashes of every file a set of posts would download (for duplicate checks).
pub fn hashes_of<'a>(posts: impl IntoIterator<Item = &'a Post>) -> Vec<String> {
    let mut seen = HashSet::new();
    posts
        .into_iter()
        .flat_map(|p| p.files.iter().filter_map(|f| f.hash.clone()))
        .filter(|h| seen.insert(h.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pawchive::normalize::normalize_post;
    use crate::settings::Settings;

    fn post() -> Post {
        let a = "a".repeat(64);
        let b = "b".repeat(64);
        normalize_post(serde_json::from_value(serde_json::json!({
            "id": "1", "user": "9", "service": "patreon", "title": "T", "published": "2026-01-02T00:00:00",
            "content": "<p>Body</p>",
            "file": {"name": "a.png", "path": format!("/aa/aa/{a}.png"), "server": "https://n1.pawchive.pw"},
            "attachments": [
                {"name": "v.mp4", "path": format!("/bb/bb/{b}.mp4"), "server": "https://n1.pawchive.pw"},
                {"name": "p.jpg", "path": format!("/cc/cc/{}.jpg", "c".repeat(64)), "preview_only": true}
            ]
        })).unwrap())
    }

    fn options(f: impl FnOnce(&mut Settings)) -> JobOptions {
        let mut s = Settings {
            download_dir: "/dl".into(),
            ..Default::default()
        };
        f(&mut s);
        JobOptions::from_settings(&s, &JobOverrides::default())
    }

    #[test]
    fn plans_individual_files_and_text() {
        let items = plan_post(&post(), None, &options(|_| {}), None, &HashSet::new(), 0);
        assert_eq!(items.len(), 4);
        assert_eq!(
            items[0].path,
            "Pawchive/9 (patreon)/2026-01-02 T [1]/01_a.png"
        );
        match &items[2].kind {
            ItemKind::File { source, .. } => {
                assert!(source.relay);
                assert!(source.url.is_none());
            }
            _ => panic!(),
        }
        assert_eq!(items[3].label, "content.txt");
        assert!(matches!(&items[3].kind, ItemKind::Text { text } if text.contains("Body")));
    }

    #[test]
    fn skips_known_files_and_previews() {
        let known: HashSet<String> = ["a".repeat(64), "b".repeat(64)].into_iter().collect();
        let opts = options(|s| s.preview_only = crate::settings::PreviewOnlyAction::Skip);
        let items = plan_post(&post(), None, &opts, None, &known, 0);
        assert_eq!(items[0].skip_reason, Some(SkipReason::Duplicate));
        assert_eq!(items[2].skip_reason, Some(SkipReason::PreviewOnly));
        let opts = options(|s| s.preview_only = crate::settings::PreviewOnlyAction::Thumbnail);
        let items = plan_post(&post(), None, &opts, Some(&[3]), &HashSet::new(), 0);
        assert_eq!(items[0].label, "p_preview.jpg");
        assert!(items[0].fallback);
    }

    #[test]
    fn plans_one_archive_per_post() {
        let opts = options(|s| {
            s.download_mode = DownloadMode::Zip;
            s.zip_videos_separately = true;
            s.save_metadata = true;
        });
        let creator = CreatorInfo {
            service: "patreon".into(),
            id: "9".into(),
            name: "Cat".into(),
        };
        let items = plan_post(&post(), Some(&creator), &opts, None, &HashSet::new(), 0);
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].path,
            "Pawchive/Cat (patreon)/Cat - 2026-01-02 T [1].zip"
        );
        match &items[0].kind {
            ItemKind::Zip { entries, .. } => {
                let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
                assert_eq!(names, ["01_a.png", "03_p.jpg", "content.txt", "post.json"]);
            }
            _ => panic!(),
        }
        assert_eq!(items[1].label, "v.mp4");
    }

    #[test]
    fn names_text_files_by_post_when_posts_share_a_folder() {
        let opts = options(|s| s.folder_template = "Pawchive/{creator}".into());
        let items = plan_post(&post(), None, &opts, None, &HashSet::new(), 0);
        let text = items.iter().find(|i| i.kind_name() == "text").unwrap();
        assert_eq!(text.path, "Pawchive/9/1_content.txt");
        assert!(per_post_folder("x/{published:YYYY} y"));
        assert!(!per_post_folder("Pawchive/{creator} ({service})"));
    }

    #[test]
    fn filters_files_and_cleans_names() {
        let opts = options(|s| {
            s.filter.skip_extensions = vec!["mp4".into()];
            s.name_remove = vec!["T".into()];
            s.folder_template = "{title}x".into();
            s.file_template = "{name}.{ext}".into();
        });
        let items = plan_post(&post(), None, &opts, None, &HashSet::new(), 0);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["a.png", "p.jpg", "content.txt"]);
        // An emptied title falls back to the post id.
        assert_eq!(items[0].path, "1x/a.png");
        // An explicit pick of files ignores the filter.
        let items = plan_post(&post(), None, &opts, Some(&[2]), &HashSet::new(), 0);
        assert_eq!(items[0].label, "v.mp4");
    }

    #[test]
    fn saves_links_and_plans_comments() {
        let mut p = post();
        p.links = vec!["https://mega.nz/folder/x".into()];
        let opts = options(|s| s.save_comments = true);
        let items = plan_post(&p, None, &opts, None, &HashSet::new(), 0);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(&labels[3..], ["content.txt", "links.txt", "comments.txt"]);
        let zip = options(|s| {
            s.save_comments = true;
            s.download_mode = DownloadMode::Zip;
        });
        let items = plan_post(&p, None, &zip, None, &HashSet::new(), 0);
        let last = items.last().unwrap();
        assert_eq!(
            last.path,
            "Pawchive/9 (patreon)/9 - 2026-01-02 T [1] - comments.txt"
        );
        let off = options(|s| s.save_links = false);
        let items = plan_post(&p, None, &off, None, &HashSet::new(), 0);
        assert!(items.iter().all(|i| i.label != "links.txt"));
    }

    #[test]
    fn nests_replies_and_finds_the_creator_folder() {
        use crate::pawchive::types::Comment;
        let c = |id: &str, parent: Option<&str>, who: &str, text: &str| Comment {
            id: id.into(),
            parent_id: parent.map(|p| serde_json::json!(p)),
            commenter_name: Some(who.into()),
            content: Some(text.into()),
            published: Some("2026-11-15T06:10:09".into()),
        };
        let text = comments_text(
            "T",
            "u",
            &[
                c("1", None, "Ann", "Nice"),
                c("2", Some("1"), "Bo", "Agreed"),
                c("3", Some("9"), "Cy", "Hi"),
            ],
        );
        assert_eq!(
            text,
            "Comments on T\nu\n\nAnn  2026-11-15 06:10:09\nNice\n\n    Bo  2026-11-15 06:10:09\n    Agreed\n\nCy  2026-11-15 06:10:09\nHi\n"
        );
        let creator = CreatorInfo {
            service: "fanbox".into(),
            id: "5".into(),
            name: "Cat".into(),
        };
        let item = announcements_item(&options(|_| {}), &creator, 0);
        assert_eq!(item.path, "Pawchive/Cat (fanbox)/announcements.txt");
    }

    #[test]
    fn thumbnail_mode_saves_previews_and_text() {
        let opts = options(|s| {
            s.download_mode = DownloadMode::Thumbnails;
            s.save_content = ContentFormat::None;
        });
        let items = plan_post(&post(), None, &opts, None, &HashSet::new(), 0);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["a_preview.png", "p_preview.jpg", "content.txt"]);
        assert_eq!(hashes_of([&post()]).len(), 3);
    }
}
