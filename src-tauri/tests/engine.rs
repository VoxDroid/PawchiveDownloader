//! Drives the real download engine against a mock Pawchive server.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pawchive_downloader_lib::core::{Core, CoreOptions};
use pawchive_downloader_lib::jobs::engine::JobAction;
use pawchive_downloader_lib::jobs::model::{
    ItemStatus, JobOverrides, JobRequest, JobStatus, JobSummary, PostRef,
};
use pawchive_downloader_lib::settings::{ContentFormat, DownloadMode, PreviewOnlyAction};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// A file Pawchive would store: content, hash path and API entry.
struct Fixture {
    bytes: Vec<u8>,
    hash: String,
    name: String,
}

impl Fixture {
    fn new(name: &str, size: usize, seed: u8) -> Self {
        let bytes: Vec<u8> = (0..size)
            .map(|i| (i as u8).wrapping_mul(31).wrapping_add(seed))
            .collect();
        Self {
            hash: sha(&bytes),
            bytes,
            name: name.into(),
        }
    }
    fn path(&self) -> String {
        format!(
            "/{}/{}/{}.bin",
            &self.hash[0..2],
            &self.hash[2..4],
            self.hash
        )
    }
    fn data_path(&self) -> String {
        format!("/data{}", self.path())
    }
    fn api(&self) -> Value {
        json!({ "name": self.name, "path": self.path(), "server": "https://n1.pawchive.pw" })
    }
}

struct Env {
    server: MockServer,
    core: Arc<Core>,
    dir: tempfile::TempDir,
}

async fn env() -> Env {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let core = Core::new(CoreOptions {
        data_dir: dir.path().join("data"),
        log_file: None,
        origin_override: Some(server.uri()),
        in_memory: true,
        fast_timing: true,
    })
    .unwrap();
    {
        let mut s = core.settings.write();
        s.download_dir = dir.path().join("out").to_string_lossy().into_owned();
        s.max_retries = 2;
        s.save_content = ContentFormat::Txt;
        s.concurrency = 3;
        // With fast timing the retry wait is in milliseconds, so the default
        // would put failed files back in the queue while a test inspects
        // them. The test for automatic retries turns them on itself.
        s.auto_retry_minutes = 0;
    }
    core.start();
    Env { server, core, dir }
}

impl Env {
    fn out(&self) -> PathBuf {
        self.dir.path().join("out")
    }

    async fn wait(&self, id: &str) -> JobSummary {
        for _ in 0..400 {
            let job = self
                .core
                .engine
                .summaries()
                .into_iter()
                .find(|j| j.id == id)
                .unwrap();
            if job.status.finished() {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        panic!(
            "job {id} did not finish: {:?}",
            self.core.engine.summaries()
        );
    }

    async fn mount_creator(&self, posts: Vec<Value>) {
        Mock::given(method("GET"))
            .and(path("/api/v1/patreon/user/7/profile"))
            .and(header("accept", "text/css"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"id": "7", "name": "Aster Vale", "service": "patreon"})),
            )
            .mount(&self.server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/patreon/user/7/posts"))
            .and(query_param("o", "0"))
            .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(posts.clone())))
            .mount(&self.server)
            .await;
        for p in posts {
            let id = p["id"].as_str().unwrap().to_string();
            Mock::given(method("GET"))
                .and(path(format!("/api/v1/patreon/user/7/post/{id}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(p))
                .mount(&self.server)
                .await;
        }
    }

    async fn serve(&self, file: &Fixture) {
        Mock::given(method("GET"))
            .and(path(file.data_path()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(file.bytes.clone()))
            .mount(&self.server)
            .await;
    }
}

fn post(id: &str, title: &str, files: &[&Fixture]) -> Value {
    json!({
        "id": id, "user": "7", "service": "patreon", "title": title,
        "content": format!("<p>Body of {title}</p>"),
        "published": "2026-05-0{}T10:00:00".replace("{}", &id[id.len() - 1..]),
        "file": files.first().map(|f| f.api()).unwrap_or(json!({})),
        "attachments": files.iter().skip(1).map(|f| f.api()).collect::<Vec<_>>(),
    })
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            } else {
                out.push(e.path());
            }
        }
    }
    out.sort();
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_a_whole_creator_and_records_it() {
    let env = env().await;
    let a = Fixture::new("a.png", 3000, 1);
    let b = Fixture::new("b.png", 5000, 2);
    let c = Fixture::new("c.png", 700, 3);
    env.mount_creator(vec![
        post("101", "First", &[&a, &b]),
        post("102", "Second", &[&c, &a]),
    ])
    .await;
    for f in [&a, &b, &c] {
        env.serve(f).await;
    }
    let id = env.core.engine.create(JobRequest::Creator {
        service: "patreon".into(),
        creator_id: "7".into(),
        query: None,
        post_ids: None,
        since: None,
        until: None,
        options: JobOverrides::default(),
    });
    let job = env.wait(&id).await;
    assert_eq!(job.status, JobStatus::Done, "{job:?}");
    assert_eq!(job.title, "Aster Vale");
    // a.png appears in both posts but is fetched once.
    assert_eq!(job.stats.skipped, 1);
    let files = files_under(&env.out());
    let names: Vec<String> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names.iter().filter(|n| n.ends_with(".png")).count(),
        3,
        "{names:?}"
    );
    assert_eq!(names.iter().filter(|n| *n == "content.txt").count(), 2);
    let first = files.iter().find(|p| p.ends_with("01_a.png")).unwrap();
    assert!(first.to_string_lossy().contains("Aster Vale (patreon)"));
    assert_eq!(std::fs::read(first).unwrap(), a.bytes);
    // Remembered for next time, and written to the history.
    assert_eq!(
        env.core
            .store
            .known_hashes(&[a.hash.clone(), c.hash.clone()])
            .len(),
        2
    );
    let history = env.core.store.list_history();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].files.done, 5);
    assert_eq!(history[0].refs.len(), 2);

    // A second run skips everything already downloaded.
    let again = env.core.engine.create(JobRequest::Refs {
        title: None,
        creator: None,
        refs: vec![PostRef {
            service: "patreon".into(),
            creator_id: "7".into(),
            post_id: "101".into(),
        }],
        options: JobOverrides {
            save_content: Some(ContentFormat::None),
            ..Default::default()
        },
    });
    let job = env.wait(&again).await;
    assert_eq!(job.stats.skipped, 2);
    assert_eq!(job.stats.done, 0);
}

/// Serves the first half of a file, then the rest only to a Range request.
struct Resumable {
    bytes: Vec<u8>,
    calls: std::sync::atomic::AtomicUsize,
}

impl Respond for Resumable {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match request.headers.get("range").and_then(|v| v.to_str().ok()) {
            Some(range) => {
                let start: usize = range
                    .trim_start_matches("bytes=")
                    .trim_end_matches('-')
                    .parse()
                    .unwrap();
                ResponseTemplate::new(206)
                    .insert_header(
                        "content-range",
                        format!(
                            "bytes {start}-{}/{}",
                            self.bytes.len() - 1,
                            self.bytes.len()
                        ),
                    )
                    .set_body_bytes(self.bytes[start..].to_vec())
            }
            None => ResponseTemplate::new(500),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn resumes_a_part_file_with_a_range_request() {
    let env = env().await;
    let f = Fixture::new("big.bin", 20_000, 9);
    env.mount_creator(vec![post("103", "Big", &[&f])]).await;
    let responder = Resumable {
        bytes: f.bytes.clone(),
        calls: Default::default(),
    };
    Mock::given(method("GET"))
        .and(path(f.data_path()))
        .respond_with(responder)
        .mount(&env.server)
        .await;
    // Half of the file is already on disk from an earlier session.
    let target_dir = env
        .out()
        .join("Pawchive/Aster Vale (patreon)/2026-05-03 Big [103]");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(target_dir.join("1_big.bin.part"), &f.bytes[..8_000]).unwrap();
    {
        let mut s = env.core.settings.write();
        s.file_template = "{index}_{filename}".into();
        s.save_content = ContentFormat::None;
        s.conflict_action = pawchive_downloader_lib::settings::ConflictAction::Overwrite;
    }
    let id = env.core.engine.create(JobRequest::Refs {
        title: None,
        creator: None,
        refs: vec![PostRef {
            service: "patreon".into(),
            creator_id: "7".into(),
            post_id: "103".into(),
        }],
        options: JobOverrides::default(),
    });
    let job = env.wait(&id).await;
    assert_eq!(
        job.status,
        JobStatus::Done,
        "{:?}",
        env.core.engine.items(&id)
    );
    let saved = std::fs::read(target_dir.join("1_big.bin")).unwrap();
    assert_eq!(saved, f.bytes);
    assert!(!target_dir.join("1_big.bin.part").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn waits_out_rate_limits_and_rejects_bad_checksums() {
    let env = env().await;
    let ok = Fixture::new("ok.bin", 1000, 4);
    let bad = Fixture::new("bad.bin", 1000, 5);
    env.mount_creator(vec![post("104", "Mixed", &[&ok, &bad])])
        .await;
    // The first request for ok.bin is rate limited.
    Mock::given(method("GET"))
        .and(path(ok.data_path()))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "0"))
        .up_to_n_times(1)
        .mount(&env.server)
        .await;
    env.serve(&ok).await;
    // bad.bin never matches its checksum.
    Mock::given(method("GET"))
        .and(path(bad.data_path()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0u8; 1000]))
        .mount(&env.server)
        .await;
    let id = env.core.engine.create(JobRequest::Refs {
        title: Some("Mixed".into()),
        creator: None,
        refs: vec![PostRef {
            service: "patreon".into(),
            creator_id: "7".into(),
            post_id: "104".into(),
        }],
        options: JobOverrides {
            save_content: Some(ContentFormat::None),
            ..Default::default()
        },
    });
    let job = env.wait(&id).await;
    assert_eq!(job.status, JobStatus::Failed);
    assert_eq!(job.stats.done, 1);
    assert_eq!(job.stats.failed, 1);
    let items = env.core.engine.items(&id);
    let failed = items
        .iter()
        .find(|i| i.status == ItemStatus::Failed)
        .unwrap();
    assert!(
        failed.error.as_deref().unwrap().contains("checksum"),
        "{failed:?}"
    );
    assert_eq!(failed.attempts, 2);
    // Retrying the job after fixing the server succeeds.
    env.server.reset().await;
    env.mount_creator(vec![post("104", "Mixed", &[&ok, &bad])])
        .await;
    env.serve(&bad).await;
    env.core.engine.act(&id, JobAction::Retry);
    let job = env.wait(&id).await;
    assert_eq!(job.status, JobStatus::Done);
}

#[tokio::test(flavor = "multi_thread")]
async fn fetches_preview_only_files_through_relay_links() {
    let env = env().await;
    let full = Fixture::new("clip.mp4", 4000, 6);
    let raw = json!({
        "id": "105", "user": "7", "service": "patreon", "title": "Preview",
        "attachments": [{ "name": "clip.mp4", "path": full.path(), "preview_only": true }]
    });
    env.mount_creator(vec![raw]).await;
    let html = format!(
        r#"<html><body><a href="https://t4.pawchive.pw/f/abc/clip.mp4?e=9999999999&amp;s=sig" download="clip.mp4"><img src="/thumbnail/data{}"></a></body></html>"#,
        full.path().replace(".bin", ".mp4")
    );
    Mock::given(method("GET"))
        .and(path("/patreon/user/7/post/105"))
        .respond_with(ResponseTemplate::new(200).set_body_string(html))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/f/abc/clip.mp4"))
        .and(query_param("s", "sig"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(full.bytes.clone()))
        .mount(&env.server)
        .await;
    {
        let mut s = env.core.settings.write();
        s.preview_only = PreviewOnlyAction::Relay;
        s.verify_hashes = true;
    }
    let id = env.core.engine.create(JobRequest::Refs {
        title: None,
        creator: None,
        refs: vec![PostRef {
            service: "patreon".into(),
            creator_id: "7".into(),
            post_id: "105".into(),
        }],
        options: JobOverrides {
            save_content: Some(ContentFormat::None),
            ..Default::default()
        },
    });
    let job = env.wait(&id).await;
    assert_eq!(
        job.status,
        JobStatus::Done,
        "{:?}",
        env.core.engine.items(&id)
    );
    let saved = files_under(&env.out());
    assert_eq!(saved.len(), 1);
    assert_eq!(std::fs::read(&saved[0]).unwrap(), full.bytes);
}

#[tokio::test(flavor = "multi_thread")]
async fn writes_one_zip_archive_per_post() {
    let env = env().await;
    let a = Fixture::new("a.png", 2000, 7);
    let b = Fixture::new("b.txt", 300, 8);
    env.mount_creator(vec![post("106", "Zipped", &[&a, &b])])
        .await;
    env.serve(&a).await;
    env.serve(&b).await;
    let id = env.core.engine.create(JobRequest::Refs {
        title: None,
        creator: None,
        refs: vec![PostRef {
            service: "patreon".into(),
            creator_id: "7".into(),
            post_id: "106".into(),
        }],
        options: JobOverrides {
            mode: Some(DownloadMode::Zip),
            ..Default::default()
        },
    });
    let job = env.wait(&id).await;
    assert_eq!(
        job.status,
        JobStatus::Done,
        "{:?}",
        env.core.engine.items(&id)
    );
    let saved = files_under(&env.out());
    assert_eq!(saved.len(), 1, "{saved:?}");
    assert!(saved[0]
        .to_string_lossy()
        .ends_with("Aster Vale - 2026-05-06 Zipped [106].zip"));
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&saved[0]).unwrap()).unwrap();
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().unwrap().to_string())
        .collect();
    assert_eq!(names, ["01_a.png", "02_b.txt", "content.txt"]);
    // The staging folder is gone.
    assert!(!env.out().join(".pawchive-staging").join(&id).exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn pauses_resumes_and_cancels() {
    let env = env().await;
    let slow = Fixture::new("slow.bin", 2000, 10);
    env.mount_creator(vec![post("107", "Slow", &[&slow])]).await;
    Mock::given(method("GET"))
        .and(path(slow.data_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(400))
                .set_body_bytes(slow.bytes.clone()),
        )
        .mount(&env.server)
        .await;
    let request = || JobRequest::Refs {
        title: None,
        creator: None,
        refs: vec![PostRef {
            service: "patreon".into(),
            creator_id: "7".into(),
            post_id: "107".into(),
        }],
        options: JobOverrides {
            save_content: Some(ContentFormat::None),
            skip_downloaded: Some(false),
            ..Default::default()
        },
    };
    let id = env.core.engine.create(request());
    tokio::time::sleep(Duration::from_millis(150)).await;
    env.core.engine.act(&id, JobAction::Pause);
    tokio::time::sleep(Duration::from_millis(600)).await;
    let paused = env
        .core
        .engine
        .summaries()
        .into_iter()
        .find(|j| j.id == id)
        .unwrap();
    assert_eq!(paused.status, JobStatus::Paused);
    assert_eq!(paused.stats.done, 0);
    env.core.engine.act(&id, JobAction::Resume);
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);

    // The file is on disk now; a second copy is what gets cancelled.
    env.core.settings.write().conflict_action =
        pawchive_downloader_lib::settings::ConflictAction::Uniquify;
    let second = env.core.engine.create(request());
    tokio::time::sleep(Duration::from_millis(100)).await;
    env.core.engine.act(&second, JobAction::Cancel);
    let job = env.wait(&second).await;
    assert_eq!(job.status, JobStatus::Cancelled);
    env.core.engine.act(&second, JobAction::Remove);
    assert!(env.core.engine.summaries().iter().all(|j| j.id != second));
    // Nothing half-written is left behind.
    assert!(files_under(&env.out())
        .iter()
        .all(|p| !p.to_string_lossy().ends_with(".part")));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_watchlist_finds_new_posts() {
    let env = env().await;
    let a = Fixture::new("a.png", 100, 11);
    env.mount_creator(vec![post("108", "Old", &[&a])]).await;
    assert!(env.core.watcher.add("patreon", "7", "Aster Vale").unwrap());
    let cancel = tokio_util::sync::CancellationToken::new();
    // The first check records a baseline.
    assert_eq!(env.core.watcher.check(None, &cancel).await.found, 0);
    env.server.reset().await;
    env.mount_creator(vec![post("109", "New", &[&a]), post("108", "Old", &[&a])])
        .await;
    let report = env.core.watcher.check(None, &cancel).await;
    assert_eq!(report.found, 1);
    assert_eq!(env.core.watcher.new_count(), 1);
    assert_eq!(env.core.watcher.list()[0].new_posts[0].id, "109");
}

// ------------------------------------------------------------- hardening

fn creator_request(options: JobOverrides) -> JobRequest {
    JobRequest::Creator {
        service: "patreon".into(),
        creator_id: "7".into(),
        query: None,
        post_ids: None,
        since: None,
        until: None,
        options,
    }
}

fn refs_request(post_id: &str, options: JobOverrides) -> JobRequest {
    JobRequest::Refs {
        title: None,
        creator: None,
        refs: vec![PostRef {
            service: "patreon".into(),
            creator_id: "7".into(),
            post_id: post_id.into(),
        }],
        options,
    }
}

fn no_text() -> JobOverrides {
    JobOverrides {
        save_content: Some(ContentFormat::None),
        ..Default::default()
    }
}

/// Regression: Tauri runs synchronous commands (and tray menu events) on the
/// main thread, which has no Tokio runtime. Creating or resuming a job that
/// needs a lookup there used to panic and close the app.
#[tokio::test(flavor = "multi_thread")]
async fn creates_and_resumes_jobs_from_threads_without_a_runtime() {
    let env = env().await;
    let a = Fixture::new("a.png", 500, 21);
    env.mount_creator(vec![post("201", "One", &[&a])]).await;
    env.serve(&a).await;
    let engine = Arc::clone(&env.core.engine);
    let id = std::thread::spawn(move || {
        assert!(tokio::runtime::Handle::try_current().is_err());
        let id = engine.create(creator_request(JobOverrides::default()));
        engine.act(&id, JobAction::Pause);
        engine.act(&id, JobAction::Resume);
        engine.act_all(JobAction::Pause);
        engine.act_all(JobAction::Resume);
        id
    })
    .join()
    .expect("no panic outside the runtime");
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
}

#[tokio::test(flavor = "multi_thread")]
async fn does_not_queue_the_same_download_twice() {
    let env = env().await;
    let a = Fixture::new("a.png", 500, 22);
    env.mount_creator(vec![post("202", "Twice", &[&a])]).await;
    Mock::given(method("GET"))
        .and(path(a.data_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(300))
                .set_body_bytes(a.bytes.clone()),
        )
        .mount(&env.server)
        .await;
    let (first, dup1) = env
        .core
        .engine
        .create_checked(creator_request(JobOverrides::default()));
    let (second, dup2) = env
        .core
        .engine
        .create_checked(creator_request(JobOverrides::default()));
    assert!(!dup1 && dup2);
    assert_eq!(first, second);
    // Different options are a different download.
    let (zip, dup3) = env
        .core
        .engine
        .create_checked(creator_request(JobOverrides {
            mode: Some(DownloadMode::Zip),
            ..Default::default()
        }));
    assert!(!dup3);
    env.wait(&first).await;
    env.wait(&zip).await;
    // Once finished, the same request may be queued again.
    let (_, dup4) = env
        .core
        .engine
        .create_checked(creator_request(JobOverrides::default()));
    assert!(!dup4);
}

#[tokio::test(flavor = "multi_thread")]
async fn files_with_the_same_name_never_share_a_target() {
    let env = env().await;
    let files: Vec<Fixture> = (0..6)
        .map(|i| Fixture::new("same.bin", 3000 + i * 10, 30 + i as u8))
        .collect();
    let refs: Vec<&Fixture> = files.iter().collect();
    env.mount_creator(vec![post("203", "Same", &refs)]).await;
    for f in &files {
        Mock::given(method("GET"))
            .and(path(f.data_path()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(100))
                    .set_body_bytes(f.bytes.clone()),
            )
            .mount(&env.server)
            .await;
    }
    {
        let mut s = env.core.settings.write();
        s.file_template = "{filename}".into();
        s.conflict_action = pawchive_downloader_lib::settings::ConflictAction::Overwrite;
        s.concurrency = 5;
        s.skip_downloaded = false;
    }
    let id = env.core.engine.create(refs_request("203", no_text()));
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    // Six different files with one name, written at the same time: with
    // "replace" none may overwrite another that is still being written.
    let saved = files_under(&env.out());
    let mut contents: Vec<Vec<u8>> = saved.iter().map(|p| std::fs::read(p).unwrap()).collect();
    contents.sort();
    contents.dedup();
    for f in &files {
        assert!(contents.contains(&f.bytes) || saved.len() < 6);
    }
    for p in &saved {
        let bytes = std::fs::read(p).unwrap();
        assert!(files.iter().any(|f| f.bytes == bytes), "mixed file {p:?}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn never_saves_an_error_page_as_a_file() {
    let env = env().await;
    let page = Fixture::new("page.html", 200, 40);
    let no_hash = json!({ "name": "nohash.jpg", "path": "/aa/bb/nohash.jpg", "server": "https://n1.pawchive.pw" });
    let raw = json!({
        "id": "204", "user": "7", "service": "patreon", "title": "Pages",
        "file": page.api(), "attachments": [no_hash]
    });
    env.mount_creator(vec![raw]).await;
    // A real HTML attachment is fine...
    Mock::given(method("GET"))
        .and(path(page.data_path()))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/html; charset=utf-8")
                .set_body_bytes(page.bytes.clone()),
        )
        .mount(&env.server)
        .await;
    // ...but an image answered with a web page is not.
    Mock::given(method("GET"))
        .and(path("/data/aa/bb/nohash.jpg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw("<html>Please sign in</html>", "text/html; charset=utf-8"),
        )
        .mount(&env.server)
        .await;
    let id = env.core.engine.create(refs_request("204", no_text()));
    let job = env.wait(&id).await;
    assert_eq!((job.stats.done, job.stats.failed), (1, 1));
    let items = env.core.engine.items(&id);
    let failed = items
        .iter()
        .find(|i| i.status == ItemStatus::Failed)
        .unwrap();
    assert!(failed.error.as_deref().unwrap().contains("web page"));
    let saved = files_under(&env.out());
    assert_eq!(saved.len(), 1);
    assert!(saved[0].to_string_lossy().ends_with("page.html"));
}

#[tokio::test(flavor = "multi_thread")]
async fn stress_a_large_creator_with_flaky_servers() {
    let env = env().await;
    // 120 posts on three pages, three files each, every file failing once.
    let mut posts = Vec::new();
    let mut fixtures = Vec::new();
    for n in 0..120u32 {
        let files: Vec<Fixture> = (0..3u32)
            .map(|i| {
                Fixture::new(
                    &format!("f{i}.bin"),
                    400 + ((n * 7 + i) % 300) as usize,
                    (n * 3 + i) as u8,
                )
            })
            .collect();
        posts.push(post(
            &format!("3{n:03}"),
            &format!("Post {n}"),
            &files.iter().collect::<Vec<_>>(),
        ));
        fixtures.extend(files);
    }
    Mock::given(method("GET"))
        .and(path("/api/v1/patreon/user/7/profile"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"id": "7", "name": "Aster Vale", "service": "patreon"})),
        )
        .mount(&env.server)
        .await;
    for (page, chunk) in posts.chunks(50).enumerate() {
        Mock::given(method("GET"))
            .and(path("/api/v1/patreon/user/7/posts"))
            .and(query_param("o", (page * 50).to_string()))
            .respond_with(ResponseTemplate::new(200).set_body_json(Value::Array(chunk.to_vec())))
            .mount(&env.server)
            .await;
    }
    let mut seen = std::collections::HashSet::new();
    for f in &fixtures {
        if !seen.insert(f.hash.clone()) {
            continue;
        }
        Mock::given(method("GET"))
            .and(path(f.data_path()))
            .respond_with(ResponseTemplate::new(503))
            .up_to_n_times(1)
            .mount(&env.server)
            .await;
        env.serve(f).await;
    }
    {
        let mut s = env.core.settings.write();
        s.concurrency = 5;
        s.save_content = ContentFormat::None;
    }
    let id = env
        .core
        .engine
        .create(creator_request(JobOverrides::default()));
    let mut job = None;
    for _ in 0..2400 {
        let j = env
            .core
            .engine
            .summaries()
            .into_iter()
            .find(|j| j.id == id)
            .unwrap();
        if j.status.finished() {
            job = Some(j);
            break;
        }
        // The queue is saved while it runs.
        env.core.engine.save();
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    let job = job.expect("finished");
    assert_eq!(job.status, JobStatus::Done, "{:?}", job.stats);
    assert_eq!(job.post_count, 120);
    assert_eq!(job.stats.done + job.stats.skipped, 360);
    assert_eq!(files_under(&env.out()).len(), seen.len());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restarted_app_finishes_the_queue() {
    let probe = env().await;
    let data = probe.dir.path().join("lib");
    let out = probe.dir.path().join("restart-out");
    let make = || {
        let core = Core::new(CoreOptions {
            data_dir: data.clone(),
            log_file: None,
            origin_override: Some(probe.server.uri()),
            in_memory: false,
            fast_timing: true,
        })
        .unwrap();
        {
            let mut s = core.settings.write();
            s.download_dir = out.to_string_lossy().into_owned();
            s.save_content = ContentFormat::None;
            s.concurrency = 1;
        }
        core
    };
    let files: Vec<Fixture> = (0..4u8)
        .map(|i| Fixture::new(&format!("r{i}.bin"), 2000, 50 + i))
        .collect();
    probe
        .mount_creator(vec![post(
            "205",
            "Restart",
            &files.iter().collect::<Vec<_>>(),
        )])
        .await;
    for f in &files {
        Mock::given(method("GET"))
            .and(path(f.data_path()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_delay(Duration::from_millis(250))
                    .set_body_bytes(f.bytes.clone()),
            )
            .mount(&probe.server)
            .await;
    }
    let first = make();
    first.start();
    let id = first
        .engine
        .create(creator_request(JobOverrides::default()));
    tokio::time::sleep(Duration::from_millis(600)).await;
    // The app closes in the middle of the job.
    first.shutdown();
    // A real restart is a new process; here both run in one, so let the
    // first one's cancelled transfers stop before the second takes over the
    // same .part files.
    for _ in 0..200 {
        if first.engine.active_count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(first.engine.active_count(), 0, "transfers stop on shutdown");
    let done_before = first
        .engine
        .items(&id)
        .iter()
        .filter(|i| i.status == ItemStatus::Done)
        .count();
    assert!(done_before < 4, "the job should still be running");
    // A new start picks the queue up and finishes it.
    let second = make();
    second.start();
    let mut finished = false;
    for _ in 0..400 {
        if let Some(j) = second.engine.summaries().into_iter().find(|j| j.id == id) {
            if j.status == JobStatus::Done {
                finished = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(
        finished,
        "{:?}
{:?}",
        second.engine.summaries(),
        second.engine.items(&id)
    );
    let saved = files_under(&out);
    assert_eq!(saved.len(), 4, "{saved:?}");
    assert!(saved
        .iter()
        .all(|p| !p.to_string_lossy().ends_with(".part")));
}

#[tokio::test(flavor = "multi_thread")]
async fn hostile_names_stay_inside_the_download_folder() {
    let env = env().await;
    let a = Fixture::new("../../escape.png", 100, 60);
    let b = Fixture::new("CON.txt", 100, 61);
    let c = Fixture::new("evil\u{202e}gnp.exe", 100, 62);
    let d = Fixture::new(&format!("{}.jpg", "\u{540d}".repeat(300)), 100, 63);
    let title = format!("..\\..\\{}<>:\"|?*", "\u{9577}".repeat(200));
    let raw = json!({
        "id": "206", "user": "7", "service": "patreon", "title": title,
        "file": a.api(), "attachments": [b.api(), c.api(), d.api()]
    });
    env.mount_creator(vec![raw]).await;
    for f in [&a, &b, &c, &d] {
        env.serve(f).await;
    }
    let id = env
        .core
        .engine
        .create(refs_request("206", JobOverrides::default()));
    assert_eq!(
        env.wait(&id).await.status,
        JobStatus::Done,
        "{:?}",
        env.core.engine.items(&id)
    );
    let out = env.out();
    let saved = files_under(&out);
    assert_eq!(saved.len(), 4);
    for p in saved {
        assert!(p.starts_with(&out), "{p:?}");
        let name = p.file_name().unwrap().to_string_lossy().into_owned();
        assert!(!name.contains('\u{202e}'));
        assert!(!name.eq_ignore_ascii_case("CON.txt"));
        assert!(name.chars().count() <= 120, "{name}");
        // Linux and macOS allow 255 bytes per name.
        for part in p.strip_prefix(&out).unwrap().components() {
            assert!(part.as_os_str().len() <= 200, "{part:?}");
        }
    }
    assert!(!env.dir.path().join("escape.png").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn filters_posts_and_files_and_limits_a_scan() {
    let env = env().await;
    let a = Fixture::new("page1.png", 300, 70);
    let b = Fixture::new("source.psd", 300, 71);
    let c = Fixture::new("page2.png", 300, 72);
    let d = Fixture::new("extra.png", 300, 73);
    let e = Fixture::new("sketch.png", 300, 74);
    // Newest first, as the API lists them.
    env.mount_creator(vec![
        post("305", "Comic chapter 3", &[&a, &b]),
        post("304", "Comic chapter 2 WIP", &[&c]),
        post("303", "Comic chapter 1", &[&d]),
        post("302", "Comic chapter 0", &[&e]),
        post("301", "Sketch dump", &[&e]),
    ])
    .await;
    for f in [&a, &b, &c, &d, &e] {
        env.serve(f).await;
    }
    let filter = pawchive_downloader_lib::jobs::filter::PostFilter {
        keywords: vec!["comic".into()],
        skip_words: vec![r"/\bwip\b/".into()],
        skip_extensions: vec!["psd".into()],
        max_posts: 2,
        ..Default::default()
    };
    let id = env.core.engine.create(creator_request(JobOverrides {
        filter: Some(filter),
        ..no_text()
    }));
    let job = env.wait(&id).await;
    assert_eq!(job.status, JobStatus::Done);
    assert_eq!(job.post_count, 2);
    let names: Vec<String> = files_under(&env.out())
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 2, "{names:?}");
    assert!(names.iter().any(|n| n.ends_with("page1.png")));
    assert!(names.iter().any(|n| n.ends_with("extra.png")));
}

#[tokio::test(flavor = "multi_thread")]
async fn saves_comments_announcements_and_outside_links() {
    let env = env().await;
    let a = Fixture::new("a.png", 200, 80);
    let mut with_links = post("402", "Pack", &[&a]);
    with_links["content"] = json!(
        "<p>Full set <a href=\"https://mega.nz/folder/abc#k\">here</a></p><p>Password: kitty</p>"
    );
    env.mount_creator(vec![with_links, post("401", "Quiet", &[])])
        .await;
    env.serve(&a).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/patreon/user/7/post/402/comments"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "1", "parent_id": null, "commenter_name": "Ann", "content": "Lovely", "published": "2026-05-02T10:00:00"},
            {"id": "2", "parent_id": "1", "commenter_name": "Bo", "content": "Agreed", "published": "2026-05-02T11:00:00"}
        ])))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/patreon/user/7/post/401/comments"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!([])))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/patreon/user/7/announcements"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"content": "New set on Friday", "added": "2026-05-01T00:00:00"}
        ])))
        .mount(&env.server)
        .await;
    let id = env.core.engine.create(creator_request(JobOverrides {
        save_comments: Some(true),
        save_announcements: Some(true),
        ..no_text()
    }));
    let job = env.wait(&id).await;
    assert_eq!(
        job.status,
        JobStatus::Done,
        "{:?}",
        env.core.engine.items(&id)
    );
    let items = env.core.engine.items(&id);
    let empty = items.iter().find(|i| i.id == "401:comments").unwrap();
    assert_eq!(empty.status, ItemStatus::Skipped);
    let read = |suffix: &str| {
        let file = files_under(&env.out())
            .into_iter()
            .find(|p| p.to_string_lossy().ends_with(suffix))
            .unwrap_or_else(|| panic!("no {suffix}"));
        std::fs::read_to_string(file).unwrap()
    };
    let comments = read("comments.txt");
    assert!(
        comments.contains("Ann") && comments.contains("    Agreed"),
        "{comments}"
    );
    let links = read("links.txt");
    assert!(
        links.contains("https://mega.nz/folder/abc#k") && links.contains("kitty"),
        "{links}"
    );
    assert!(read("announcements.txt").contains("New set on Friday"));
    let folder = files_under(&env.out())
        .into_iter()
        .find(|p| p.ends_with("announcements.txt"))
        .unwrap();
    assert!(folder.parent().unwrap().ends_with("Aster Vale (patreon)"));
}

#[tokio::test(flavor = "multi_thread")]
async fn searches_the_creator_list_and_keeps_it_on_disk() {
    use pawchive_downloader_lib::pawchive::directory::Sort;
    let env = env().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/creators"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"id": "7", "name": "Aster Vale", "service": "patreon", "favorited": 40, "updated": 1794960000},
            {"id": "8", "name": "Vale Studio", "service": "fanbox", "favorited": 90, "updated": 1794000000}
        ])))
        .expect(1)
        .mount(&env.server)
        .await;
    let cancel = tokio_util::sync::CancellationToken::new();
    let dir = env
        .core
        .directory(pawchive_downloader_lib::pawchive::urls::Site::Pawchive);
    dir.ensure(&env.core.client, 1_000, false, &cancel)
        .await
        .unwrap();
    // Fresh for a day: no second request.
    dir.ensure(&env.core.client, 2_000, false, &cancel)
        .await
        .unwrap();
    let found = dir.search("vale", None, Sort::Relevance, 10);
    assert_eq!(found.total, 2);
    assert_eq!(found.creators[0].name, "Vale Studio");
    assert_eq!(found.creators[1].updated, 1_794_960_000_000);
    // A new app start reads the saved list instead of the site.
    let again =
        pawchive_downloader_lib::pawchive::directory::Directory::new(env.dir.path().join("data"));
    again
        .ensure(&env.core.client, 3_000, false, &cancel)
        .await
        .unwrap();
    assert_eq!(again.search("aster", None, Sort::Relevance, 10).total, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn holds_downloads_for_a_full_disk_and_outside_the_window() {
    use pawchive_downloader_lib::jobs::engine::Hold;
    let env = env().await;
    let a = Fixture::new("a.png", 300, 90);
    env.mount_creator(vec![post("501", "One", &[&a])]).await;
    env.serve(&a).await;
    env.core.settings.write().min_free_space_mb = u32::MAX;
    let mut held = None;
    for _ in 0..200 {
        held = env.core.engine.hold();
        if held.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(matches!(held, Some(Hold::Disk { .. })), "{held:?}");
    let id = env.core.engine.create(refs_request("501", no_text()));
    tokio::time::sleep(Duration::from_millis(100)).await;
    let job = env
        .core
        .engine
        .summaries()
        .into_iter()
        .find(|j| j.id == id)
        .unwrap();
    assert_eq!(job.stats.done, 0, "nothing starts while held");

    // A closed download window holds too.
    let now = chrono::Local::now();
    {
        let mut s = env.core.settings.write();
        s.min_free_space_mb = 0;
        s.schedule_enabled = true;
        s.schedule_start = (now + chrono::Duration::hours(2))
            .format("%H:%M")
            .to_string();
        s.schedule_end = (now + chrono::Duration::hours(3))
            .format("%H:%M")
            .to_string();
    }
    for _ in 0..200 {
        if matches!(env.core.engine.hold(), Some(Hold::Schedule { .. })) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(matches!(
        env.core.engine.hold(),
        Some(Hold::Schedule { .. })
    ));
    env.core.settings.write().schedule_enabled = false;
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    assert_eq!(env.core.engine.hold(), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_the_when_done_action_after_the_queue_empties() {
    use pawchive_downloader_lib::power::WhenDone;
    use std::sync::atomic::{AtomicBool, Ordering};
    let env = env().await;
    let a = Fixture::new("a.png", 300, 91);
    env.mount_creator(vec![post("502", "One", &[&a])]).await;
    env.serve(&a).await;
    let quit = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&quit);
    env.core
        .set_on_quit(Arc::new(move || flag.store(true, Ordering::SeqCst)));
    env.core.set_when_done(WhenDone::Quit);
    let id = env.core.engine.create(refs_request("502", no_text()));
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    for _ in 0..300 {
        if quit.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(quit.load(Ordering::SeqCst), "the app was asked to quit");
    assert_eq!(env.core.queue_status().when_done, WhenDone::Nothing);
}

#[tokio::test(flavor = "multi_thread")]
async fn finds_and_restores_deleted_files() {
    use pawchive_downloader_lib::library;
    let env = env().await;
    let a = Fixture::new("keep.png", 300, 100);
    let b = Fixture::new("lost.png", 300, 101);
    env.mount_creator(vec![post("601", "Set", &[&a, &b])]).await;
    env.serve(&a).await;
    env.serve(&b).await;
    let id = env.core.engine.create(refs_request("601", no_text()));
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    let lost = files_under(&env.out())
        .into_iter()
        .find(|p| p.to_string_lossy().ends_with("lost.png"))
        .unwrap();
    std::fs::remove_file(&lost).unwrap();
    let report = library::check(&env.core.store.list_known_files());
    assert_eq!((report.checked, report.missing_files), (2, 1));
    assert_eq!(report.posts[0].post, "patreon:7:601");
    library::forget(&env.core.store, &report.posts).unwrap();
    let again = env.core.engine.create(refs_request("601", no_text()));
    let job = env.wait(&again).await;
    assert_eq!(job.status, JobStatus::Done);
    assert_eq!(
        (job.stats.done, job.stats.skipped),
        (1, 1),
        "only the lost file again"
    );
    assert!(lost.exists());
    assert_eq!(
        library::check(&env.core.store.list_known_files()).missing_files,
        0
    );
}

/// Deleting downloaded files used to leave them "downloaded" for good: the
/// library remembered them and skipped them until it was cleared by hand.
#[tokio::test(flavor = "multi_thread")]
async fn deleted_files_download_again_by_themselves() {
    let env = env().await;
    let a = Fixture::new("kept.png", 300, 110);
    let b = Fixture::new("gone.png", 300, 111);
    env.mount_creator(vec![post("611", "Pair", &[&a, &b])])
        .await;
    env.serve(&a).await;
    env.serve(&b).await;
    let id = env.core.engine.create(refs_request("611", no_text()));
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    let gone = files_under(&env.out())
        .into_iter()
        .find(|p| p.to_string_lossy().ends_with("gone.png"))
        .unwrap();
    std::fs::remove_file(&gone).unwrap();

    // No repair by hand: the missing file is fetched again, the other kept.
    let again = env.core.engine.create(refs_request("611", no_text()));
    let job = env.wait(&again).await;
    assert_eq!(job.status, JobStatus::Done);
    assert_eq!(
        (job.stats.done, job.stats.skipped, job.stats.already),
        (1, 1, 1)
    );
    assert_eq!(std::fs::read(&gone).unwrap(), b.bytes);

    // All there: nothing is fetched and both count as already downloaded.
    let third = env.core.engine.create(refs_request("611", no_text()));
    let job = env.wait(&third).await;
    assert_eq!((job.stats.done, job.stats.already), (0, 2));

    // Download again on request replaces them instead of adding copies.
    std::fs::write(&gone, b"damaged").unwrap();
    env.core.engine.act(&third, JobAction::Redownload);
    let job = env.wait(&third).await;
    assert_eq!(job.status, JobStatus::Done);
    assert_eq!((job.stats.done, job.stats.already), (2, 0));
    assert_eq!(std::fs::read(&gone).unwrap(), b.bytes);
    let pngs = files_under(&env.out())
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e == "png"))
        .count();
    assert_eq!(pngs, 2, "replaced, not numbered");
}

/// Skip is the default for a name that is already taken: a file on disk the
/// library forgot is not saved a second time as "name (1)".
#[tokio::test(flavor = "multi_thread")]
async fn a_file_already_in_the_folder_is_skipped_not_copied() {
    let env = env().await;
    let a = Fixture::new("here.png", 300, 120);
    env.mount_creator(vec![post("621", "Here", &[&a])]).await;
    env.serve(&a).await;
    let id = env.core.engine.create(refs_request("621", no_text()));
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    env.core.store.forget_known().unwrap();
    let again = env.core.engine.create(refs_request("621", no_text()));
    let job = env.wait(&again).await;
    assert_eq!(job.status, JobStatus::Done);
    assert_eq!((job.stats.done, job.stats.already), (0, 1));
    assert_eq!(files_under(&env.out()).len(), 1);
    // It is the same file (same content), so the library knows it again and
    // the Library page shows it.
    let files = env.core.store.list_known_files_with_time();
    let g = pawchive_downloader_lib::gallery::build(&files, &[], &[], &[], |p| {
        std::fs::metadata(p).ok().map(|m| m.len())
    });
    assert_eq!(g.images, 1, "a file already on disk shows in the Library");
    assert!(!env
        .core
        .store
        .known_hashes(std::slice::from_ref(&a.hash))
        .is_empty());
}

/// A different file under the same name is skipped too, but never taken for
/// the one the post has.
#[tokio::test(flavor = "multi_thread")]
async fn a_different_file_with_the_same_name_is_not_recorded() {
    let env = env().await;
    let a = Fixture::new("same.png", 300, 125);
    env.mount_creator(vec![post("622", "Same", &[&a])]).await;
    env.serve(&a).await;
    let id = env.core.engine.create(refs_request("622", no_text()));
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    let saved = files_under(&env.out()).pop().unwrap();
    std::fs::write(&saved, b"something else entirely").unwrap();
    env.core.store.forget_known().unwrap();
    let again = env.core.engine.create(refs_request("622", no_text()));
    let job = env.wait(&again).await;
    assert_eq!((job.stats.done, job.stats.skipped), (0, 1));
    assert!(env
        .core
        .store
        .known_hashes(std::slice::from_ref(&a.hash))
        .is_empty());
}

/// Files in the download folder that the library has no record of (another
/// install, a cleared history...) still show in the Library, under the right
/// post and creator, read from the folder names.
#[tokio::test(flavor = "multi_thread")]
async fn the_library_shows_files_found_in_the_download_folder() {
    use pawchive_downloader_lib::gallery;
    let env = env().await;
    let a = Fixture::new("found.png", 300, 140);
    let b = Fixture::new("also.png", 300, 141);
    env.mount_creator(vec![post("641", "Found Set", &[&a, &b])])
        .await;
    env.serve(&a).await;
    env.serve(&b).await;
    let id = env.core.engine.create(refs_request("641", no_text()));
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    // Nothing recorded and no history: as on a fresh install.
    env.core.store.forget_known().unwrap();
    env.core.store.clear_history(true).unwrap();
    let found = gallery::scan(&gallery::roots(vec![env.out()]));
    assert_eq!(found.len(), 2);
    let known = vec![(
        "patreon".to_string(),
        "7".to_string(),
        "Aster Vale".to_string(),
    )];
    let g = gallery::build(&[], &[], &known, &found, |_| None);
    assert_eq!((g.images, g.posts.len(), g.found.len()), (2, 1, 2));
    let p = &g.posts[0];
    assert_eq!(p.title, "Found Set");
    assert_eq!(
        (
            p.service.as_str(),
            p.creator_id.as_str(),
            p.post_id.as_str()
        ),
        ("patreon", "7", "641")
    );
    assert_eq!(p.creator_name, "Aster Vale");
    assert_eq!(p.key, "patreon:7:641");
    // Without anything known about the creator it still groups, by folder.
    let g = gallery::build(&[], &[], &[], &found, |_| None);
    assert_eq!(g.posts[0].creator_name, "Aster Vale");
    assert_eq!(g.posts[0].creator_id, "");
    assert_eq!(g.creators.len(), 1);
}

/// What the Library shows comes straight from what was downloaded.
#[tokio::test(flavor = "multi_thread")]
async fn downloaded_images_and_videos_appear_in_the_library() {
    use pawchive_downloader_lib::gallery::{self, MediaKind};
    let env = env().await;
    let pic = Fixture::new("page.png", 400, 130);
    let clip = Fixture::new("clip.mp4", 900, 131);
    let doc = Fixture::new("notes.pdf", 300, 132);
    env.mount_creator(vec![post("631", "Ink Set", &[&pic, &clip, &doc])])
        .await;
    for f in [&pic, &clip, &doc] {
        env.serve(f).await;
    }
    let id = env.core.engine.create(refs_request("631", no_text()));
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    let files = env.core.store.list_known_files_with_time();
    let history = env.core.store.list_history();
    let g = gallery::build(&files, &history, &[], &[], |p| {
        std::fs::metadata(p).ok().map(|m| m.len())
    });
    assert_eq!((g.images, g.videos), (1, 1), "the PDF is not media");
    assert_eq!(g.posts.len(), 1);
    assert_eq!(g.posts[0].title, "Ink Set");
    assert_eq!(g.posts[0].creator_name, "Aster Vale");
    let video = g.posts[0]
        .files
        .iter()
        .find(|f| f.kind == MediaKind::Video)
        .unwrap();
    // The protocol finds the file by its hash and hands out pieces of it.
    let path = env.core.store.path_of(&video.hash).unwrap();
    let part = gallery::serve_file(std::path::Path::new(&path), Some("bytes=0-99"));
    assert_eq!(part.status, 206);
    assert_eq!(part.body, clip.bytes[..100]);
}

#[tokio::test(flavor = "multi_thread")]
async fn watchlist_downloads_can_filter_posts() {
    let env = env().await;
    let a = Fixture::new("a.png", 300, 102);
    let b = Fixture::new("b.png", 300, 103);
    env.mount_creator(vec![
        post("701", "Final art", &[&a]),
        post("702", "Rough WIP", &[&b]),
    ])
    .await;
    env.serve(&a).await;
    env.serve(&b).await;
    let filter = pawchive_downloader_lib::jobs::filter::PostFilter {
        skip_words: vec!["wip".into()],
        ..Default::default()
    };
    let id = env.core.engine.create(JobRequest::Refs {
        title: None,
        creator: None,
        refs: ["701", "702"]
            .iter()
            .map(|p| PostRef {
                service: "patreon".into(),
                creator_id: "7".into(),
                post_id: (*p).into(),
            })
            .collect(),
        options: JobOverrides {
            filter: Some(filter),
            apply_post_filter: Some(true),
            ..no_text()
        },
    });
    let job = env.wait(&id).await;
    assert_eq!(job.status, JobStatus::Done);
    assert_eq!(job.post_count, 1);
    assert_eq!(files_under(&env.out()).len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_down_media_server_falls_back_to_the_thumbnail_at_once() {
    use pawchive_downloader_lib::jobs::model::SelectedPost;
    use pawchive_downloader_lib::pawchive::types::{FileCategory, FileRole, Post, PostFile};
    let env = env().await;
    // The site answers; its media server (port 1) refuses every connection.
    let dead = "https://127.0.0.1:1";
    env.core.client.note_site_ok(dead);
    Mock::given(method("GET"))
        .and(path("/thumb.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"thumbnail".to_vec()))
        .mount(&env.server)
        .await;
    let file = |i: u32| PostFile {
        index: i,
        role: FileRole::Attachment,
        name: format!("full{i}.png"),
        ext: "png".into(),
        hash: None,
        path: format!("/x/{i}.png"),
        url: Some(format!("{dead}/data/x/{i}.png")),
        thumbnail_url: Some(format!("{}/thumb.png", env.server.uri())),
        preview_only: false,
        category: FileCategory::Image,
    };
    let post = Post {
        id: "801".into(),
        service: "patreon@kemono".into(),
        creator_id: "7".into(),
        title: "Down".into(),
        content_html: String::new(),
        published: None,
        added: None,
        edited: None,
        tags: vec![],
        files: vec![file(1), file(2)],
        url: "https://kemono.cr/patreon/user/7/post/801".into(),
        next_id: None,
        prev_id: None,
        incomplete: false,
        links: vec![],
        passwords: vec![],
    };
    env.core.settings.write().thumbnail_fallback = true;
    let started = std::time::Instant::now();
    let id = env.core.engine.create(JobRequest::Posts {
        title: None,
        creator: None,
        posts: vec![SelectedPost {
            post,
            file_indexes: None,
        }],
        options: no_text(),
    });
    let job = env.wait(&id).await;
    assert_eq!(
        job.status,
        JobStatus::Done,
        "{:?}",
        env.core.engine.items(&id)
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "no waiting for the network"
    );
    let items = env.core.engine.items(&id);
    assert!(items.iter().all(|i| i.fallback), "{items:?}");
    assert_eq!(env.core.queue_status().down_sites, ["127.0.0.1:1"]);
    let saved = files_under(&env.out());
    assert_eq!(saved.len(), 2);
    assert!(saved
        .iter()
        .all(|p| p.to_string_lossy().ends_with("_preview.png")));
    // The folder template names the site.
    assert!(saved[0].to_string_lossy().contains("Kemono"));
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_files_are_tried_again_later() {
    let env = env().await;
    let a = Fixture::new("late.png", 300, 110);
    env.mount_creator(vec![post("901", "Late", &[&a])]).await;
    // The file fails until the server is fixed.
    Mock::given(method("GET"))
        .and(path(a.data_path()))
        .respond_with(ResponseTemplate::new(404))
        .up_to_n_times(2)
        .with_priority(1)
        .mount(&env.server)
        .await;
    env.serve(&a).await;
    env.core.settings.write().auto_retry_minutes = 0;
    let id = env.core.engine.create(refs_request("901", no_text()));
    let job = env.wait(&id).await;
    assert_eq!(job.stats.failed, 1);
    // Automatic retries come back once the wait is over (milliseconds here).
    env.core.settings.write().auto_retry_minutes = 15;
    let mut done = false;
    for _ in 0..400 {
        let j = env
            .core
            .engine
            .summaries()
            .into_iter()
            .find(|j| j.id == id)
            .unwrap();
        if j.status == JobStatus::Done && j.stats.done == 1 {
            done = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(done, "{:?}", env.core.engine.items(&id));
}

#[tokio::test(flavor = "multi_thread")]
async fn downloads_outside_links_and_unpacks_them_with_the_post_password() {
    use pawchive_downloader_lib::jobs::model::SelectedPost;
    use pawchive_downloader_lib::pawchive::types::Post;
    use std::io::Write;
    let env = env().await;
    // A password-protected archive on an outside host.
    let mut zip_bytes = Vec::new();
    {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
        let opts = zip::write::SimpleFileOptions::default()
            .with_aes_encryption(zip::AesMode::Aes256, "kitty");
        w.start_file("art/page.txt", opts).unwrap();
        w.write_all(b"full resolution").unwrap();
        w.finish().unwrap();
    }
    Mock::given(method("GET"))
        .and(path("/share/pack.zip"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(zip_bytes.clone(), "application/zip"))
        .mount(&env.server)
        .await;
    {
        let mut s = env.core.settings.write();
        s.download_outside_links = true;
        s.extract_archives = "delete".into();
    }
    let post = Post {
        id: "951".into(),
        service: "patreon".into(),
        creator_id: "7".into(),
        title: "Pack".into(),
        content_html: String::new(),
        published: None,
        added: None,
        edited: None,
        tags: vec![],
        files: vec![],
        url: "https://pawchive.pw/patreon/user/7/post/951".into(),
        next_id: None,
        prev_id: None,
        incomplete: false,
        links: vec![format!("{}/share/pack.zip", env.server.uri())],
        passwords: vec!["wrong".into(), "kitty".into()],
    };
    let id = env.core.engine.create(JobRequest::Posts {
        title: None,
        creator: None,
        posts: vec![SelectedPost {
            post,
            file_indexes: None,
        }],
        options: no_text(),
    });
    let job = env.wait(&id).await;
    assert_eq!(
        job.status,
        JobStatus::Done,
        "{:?}",
        env.core.engine.items(&id)
    );
    let saved = files_under(&env.out());
    let page = saved
        .iter()
        .find(|p| p.ends_with("page.txt"))
        .unwrap_or_else(|| panic!("{saved:?}"));
    assert_eq!(std::fs::read(page).unwrap(), b"full resolution");
    assert!(page.to_string_lossy().contains("Links"));
    assert!(
        saved.iter().all(|p| !p.to_string_lossy().ends_with(".zip")),
        "the ZIP is removed"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_paired_browser_extension_can_queue_downloads() {
    let env = env().await;
    let a = Fixture::new("from-browser.png", 300, 120);
    env.mount_creator(vec![post("991", "Sent", &[&a])]).await;
    env.serve(&a).await;
    env.core.settings.write().bridge_enabled = true;
    let mut port = None;
    for _ in 0..200 {
        port = env.core.bridge.status(&env.core).port;
        if port.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let base = format!("http://127.0.0.1:{}", port.expect("the bridge listens"));
    let http = reqwest::Client::new();
    let ext = "chrome-extension://abcdefghijklmnop";

    // A web page may not use it.
    let evil = http
        .get(format!("{base}/v1/hello"))
        .header("Origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(evil.status(), 403);
    let hello: Value = http
        .get(format!("{base}/v1/hello"))
        .header("Origin", ext)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hello["paired"], false);

    // Pairing needs the code shown in the app, once.
    let wrong = http
        .post(format!("{base}/v1/pair"))
        .header("Origin", ext)
        .json(&json!({"code": "000000x", "name": "Test"}))
        .send()
        .await
        .unwrap();
    assert_eq!(wrong.status(), 401);
    let (code, _) = env.core.bridge.start_pairing();
    let token = http
        .post(format!("{base}/v1/pair"))
        .header("Origin", ext)
        .json(&json!({"code": code, "name": "Chrome"}))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["token"]
        .as_str()
        .unwrap()
        .to_string();
    let again = http
        .post(format!("{base}/v1/pair"))
        .header("Origin", ext)
        .json(&json!({"code": code}))
        .send()
        .await
        .unwrap();
    assert_eq!(again.status(), 401, "a code works once");

    // Without the token nothing is queued; with it the job runs.
    let request = json!({"kind": "refs", "refs": [{"service": "patreon", "creatorId": "7", "postId": "991"}]});
    let anonymous = http
        .post(format!("{base}/v1/jobs"))
        .header("Origin", ext)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);
    let created: Value = http
        .post(format!("{base}/v1/jobs"))
        .header("Origin", ext)
        .bearer_auth(&token)
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["jobId"].as_str().unwrap().to_string();
    assert_eq!(env.wait(&id).await.status, JobStatus::Done);
    let jobs: Value = http
        .get(format!("{base}/v1/jobs"))
        .header("Origin", ext)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(jobs[0]["id"], id);
    let status = env.core.bridge.status(&env.core);
    assert_eq!(status.pairings.len(), 1);
    assert_eq!(status.pairings[0].name, "Chrome");
    assert!(
        status.pairings[0].last_used_at.is_some(),
        "the app sees when the browser last checked in"
    );

    // A code can be taken away before it runs out.
    env.core.bridge.start_pairing();
    env.core.bridge.cancel_pairing();
    assert_eq!(env.core.bridge.status(&env.core).code, None);

    // Unpairing in the extension removes the pairing in the app as well.
    let anonymous = http
        .post(format!("{base}/v1/unpair"))
        .header("Origin", ext)
        .send()
        .await
        .unwrap();
    assert_eq!(anonymous.status(), 401);
    let unpaired = http
        .post(format!("{base}/v1/unpair"))
        .header("Origin", ext)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(unpaired.status(), 200);
    assert!(env.core.bridge.status(&env.core).pairings.is_empty());
    let after = http
        .post(format!("{base}/v1/jobs"))
        .header("Origin", ext)
        .bearer_auth(&token)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), 401, "the old token no longer works");
}
