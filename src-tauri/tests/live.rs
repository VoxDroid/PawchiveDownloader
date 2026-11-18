//! Checks against the real pawchive.pw. Ignored by default so CI never
//! touches the site; run by hand with
//!   cargo test --test live -- --ignored --test-threads=1
//! Each test makes a handful of requests through the app's polite client.

use std::time::Duration;

use pawchive_downloader_lib::net::http::build_client;
use pawchive_downloader_lib::net::limiter::SiteLimiter;
use pawchive_downloader_lib::pawchive::client::PawchiveClient;
use pawchive_downloader_lib::pawchive::directory::{Directory, Sort};
use tokio_util::sync::CancellationToken;

/// A creator known to have comments, announcements and a fanbox account.
const SERVICE: &str = "fanbox";
const CREATOR: &str = "1176892";
const POST: &str = "12929940";

fn client() -> PawchiveClient {
    let http = build_client(&Default::default()).unwrap();
    let limiter = SiteLimiter::new(2, Duration::from_millis(1000));
    let client = PawchiveClient::new(http, limiter);
    client.configure(2, Duration::from_millis(1000));
    client
}

#[tokio::test]
#[ignore]
async fn live_creator_list_search() {
    let dir = tempfile::tempdir().unwrap();
    let directory = Directory::new(dir.path().to_path_buf());
    let cancel = CancellationToken::new();
    directory
        .ensure(
            &client(),
            chrono::Utc::now().timestamp_millis(),
            false,
            &cancel,
        )
        .await
        .unwrap();
    let all = directory.search("", None, Sort::Favorited, 5);
    println!(
        "{} creators; most favorited: {:?}",
        all.size,
        all.creators
            .iter()
            .map(|c| (&c.name, c.favorited))
            .collect::<Vec<_>>()
    );
    assert!(all.size > 10_000);
    let found = directory.search(CREATOR, None, Sort::Relevance, 5);
    assert_eq!(found.creators[0].id, CREATOR);
    assert!(dir.path().join("creators.json").exists());
}

#[tokio::test]
#[ignore]
async fn live_comments_announcements_and_links() {
    let c = client();
    let cancel = CancellationToken::new();
    let comments = c
        .get_comments(SERVICE, CREATOR, POST, &cancel)
        .await
        .unwrap();
    println!("{} comments", comments.len());
    assert!(comments.iter().all(|x| !x.id.is_empty()));
    let announcements = c
        .get_announcements(SERVICE, CREATOR, &cancel)
        .await
        .unwrap();
    println!("{} announcements", announcements.len());
    assert!(!announcements.is_empty());
    let linked = c.get_linked(SERVICE, CREATOR, &cancel).await.unwrap();
    println!("{} linked accounts", linked.len());
    let post = c.get_post(SERVICE, CREATOR, POST, &cancel).await.unwrap();
    println!("links {:?} passwords {:?}", post.links, post.passwords);
}
