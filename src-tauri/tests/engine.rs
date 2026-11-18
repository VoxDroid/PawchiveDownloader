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
        .map(|i| zip.by_index(i).unwrap().name().to_string())
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
