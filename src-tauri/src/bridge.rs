//! The browser extension bridge (docs/BRIDGE.md): a small HTTP server on
//! 127.0.0.1 that lets a paired browser extension hand downloads to the app.
//!
//! Off by default. It answers only on the loopback address, only requests
//! from browser-extension origins (so web pages cannot use it) with a Host of
//! 127.0.0.1 or localhost (against DNS rebinding), and every call except
//! `hello` and `pair` needs a token from pairing. Only token hashes are kept.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path as UrlPath, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

use crate::core::Core;
use crate::jobs::engine::{now_ms, JobAction};
use crate::jobs::model::JobRequest;

pub const PORTS: [u16; 3] = [49517, 49518, 49519];
const PAIRING_KEY: &str = "bridge.pairings";
const CODE_LIFETIME_MS: i64 = 2 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pairing {
    pub id: String,
    pub name: String,
    pub created_at: i64,
    #[serde(default)]
    pub last_used_at: Option<i64>,
    /// SHA-256 of the token, hex.
    token_hash: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BridgeStatus {
    pub running: bool,
    pub port: Option<u16>,
    pub pairings: Vec<Pairing>,
    /// The pairing code on show, and until when (epoch ms).
    pub code: Option<String>,
    pub code_expires_at: Option<i64>,
}

#[derive(Default)]
struct Inner {
    port: Option<u16>,
    stop: Option<CancellationToken>,
    code: Option<(String, i64)>,
}

#[derive(Default)]
pub struct Bridge {
    inner: Mutex<Inner>,
}

fn hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn is_extension_origin(origin: &str) -> bool {
    [
        "chrome-extension://",
        "moz-extension://",
        "extension://",
        "safari-web-extension://",
    ]
    .iter()
    .any(|p| origin.starts_with(p))
}

fn is_loopback_host(host: &str) -> bool {
    let name = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    matches!(name, "127.0.0.1" | "localhost")
}

impl Bridge {
    pub fn pairings(core: &Core) -> Vec<Pairing> {
        core.store
            .get::<Vec<Pairing>>(PAIRING_KEY)
            .unwrap_or_default()
    }

    pub fn status(&self, core: &Core) -> BridgeStatus {
        let inner = self.inner.lock();
        let code = inner.code.clone().filter(|(_, until)| *until > now_ms());
        BridgeStatus {
            running: inner.port.is_some(),
            port: inner.port,
            pairings: Self::pairings(core),
            code: code.as_ref().map(|c| c.0.clone()),
            code_expires_at: code.map(|c| c.1),
        }
    }

    /// Shows a new six-digit code for two minutes.
    pub fn start_pairing(&self) -> (String, i64) {
        let code = format!("{:06}", rand::random::<u32>() % 1_000_000);
        let until = now_ms() + CODE_LIFETIME_MS;
        self.inner.lock().code = Some((code.clone(), until));
        (code, until)
    }

    /// Takes the code on show away before it runs out.
    pub fn cancel_pairing(&self) {
        self.inner.lock().code = None;
    }

    pub fn revoke(core: &Core, id: &str) -> Result<(), String> {
        let mut list = Self::pairings(core);
        list.retain(|p| p.id != id);
        core.store.set(PAIRING_KEY, &list)
    }

    fn check_token(core: &Core, headers: &HeaderMap) -> bool {
        Self::pairing_for(core, headers).is_some()
    }

    /// The pairing a request's token belongs to; notes that it was used, so
    /// the app can show which browsers are connected.
    fn pairing_for(core: &Core, headers: &HeaderMap) -> Option<Pairing> {
        let token = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))?;
        let wanted = hash(token.trim());
        let mut list = Self::pairings(core);
        let p = list.iter_mut().find(|p| p.token_hash == wanted)?;
        p.last_used_at = Some(now_ms());
        let found = p.clone();
        let _ = core.store.set(PAIRING_KEY, &list);
        Some(found)
    }

    /// Starts or stops the server to match the setting.
    pub async fn apply(self: &Arc<Self>, core: Arc<Core>, enabled: bool) {
        let running = self.inner.lock().port.is_some();
        if enabled == running {
            return;
        }
        if !enabled {
            let mut inner = self.inner.lock();
            if let Some(stop) = inner.stop.take() {
                stop.cancel();
            }
            inner.port = None;
            core.log.info("Browser extension bridge stopped");
            return;
        }
        for port in PORTS {
            let addr = SocketAddr::from(([127, 0, 0, 1], port));
            let Ok(listener) = tokio::net::TcpListener::bind(addr).await else {
                continue;
            };
            let stop = CancellationToken::new();
            {
                let mut inner = self.inner.lock();
                inner.port = Some(port);
                inner.stop = Some(stop.clone());
            }
            let app = router(Arc::clone(self), Arc::clone(&core));
            let log = Arc::clone(&core.log);
            tokio::spawn(async move {
                let served = axum::serve(listener, app)
                    .with_graceful_shutdown(async move { stop.cancelled().await })
                    .await;
                if let Err(e) = served {
                    log.error("The browser extension bridge stopped", Some(e.to_string()));
                }
            });
            core.log.info(format!(
                "Browser extension bridge listening on 127.0.0.1:{port}"
            ));
            return;
        }
        core.log.error(
            "The browser extension bridge could not start",
            Some("ports 49517 to 49519 are in use".into()),
        );
    }
}

#[derive(Clone)]
struct Ctx {
    bridge: Arc<Bridge>,
    core: Arc<Core>,
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(serde_json::json!({ "error": code }))).into_response()
}

/// Every request: loopback Host, extension Origin, CORS for that origin.
async fn guard(
    headers: HeaderMap,
    method: Method,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let host_ok = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .is_some_and(is_loopback_host);
    let origin = headers
        .get("origin")
        .and_then(|h| h.to_str().ok())
        .map(str::to_string);
    // No Origin at all means not a browser page (curl, tests); a web page
    // always sends one, so only extension origins pass.
    let origin_ok = origin.as_deref().is_none_or(is_extension_origin);
    if !host_ok || !origin_ok {
        return error(StatusCode::FORBIDDEN, "forbidden");
    }
    let mut response = if method == Method::OPTIONS {
        StatusCode::NO_CONTENT.into_response()
    } else {
        next.run(request).await
    };
    if let Some(origin) = origin.and_then(|o| HeaderValue::from_str(&o).ok()) {
        let h = response.headers_mut();
        h.insert("access-control-allow-origin", origin);
        h.insert(
            "access-control-allow-headers",
            HeaderValue::from_static("authorization, content-type"),
        );
        h.insert(
            "access-control-allow-methods",
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
    }
    response
}

fn router(bridge: Arc<Bridge>, core: Arc<Core>) -> Router {
    Router::new()
        .route("/v1/hello", get(hello))
        .route("/v1/pair", post(pair))
        .route("/v1/unpair", post(unpair))
        .route("/v1/jobs", get(list_jobs).post(create_job))
        .route("/v1/jobs/{id}/{action}", post(job_action))
        .route("/v1/watch", post(watch))
        .layer(axum::middleware::from_fn(guard))
        .with_state(Ctx { bridge, core })
}

async fn hello(State(ctx): State<Ctx>, headers: HeaderMap) -> Response {
    let paired = Bridge::check_token(&ctx.core, &headers);
    Json(serde_json::json!({
        "app": "Pawchive Downloader",
        "version": env!("CARGO_PKG_VERSION"),
        "paired": paired,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct PairBody {
    code: String,
    #[serde(default)]
    name: String,
}

async fn pair(State(ctx): State<Ctx>, Json(body): Json<PairBody>) -> Response {
    let valid = {
        let mut inner = ctx.bridge.inner.lock();
        let ok = inner
            .code
            .as_ref()
            .is_some_and(|(code, until)| *until > now_ms() && *code == body.code.trim());
        if ok {
            // A code works once.
            inner.code = None;
        }
        ok
    };
    if !valid {
        return error(StatusCode::UNAUTHORIZED, "bad-code");
    }
    let token = hex::encode(rand::random::<[u8; 32]>());
    let mut list = Bridge::pairings(&ctx.core);
    let name: String = body.name.trim().chars().take(60).collect();
    list.push(Pairing {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.is_empty() {
            "Browser extension".into()
        } else {
            name
        },
        created_at: now_ms(),
        // Pairing is the browser talking to the app, so it counts as seen.
        last_used_at: Some(now_ms()),
        token_hash: hash(&token),
    });
    if let Err(e) = ctx.core.store.set(PAIRING_KEY, &list) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e);
    }
    ctx.core.log.info("A browser extension was paired");
    Json(serde_json::json!({ "token": token })).into_response()
}

/// The extension forgets the app: the app forgets the extension too.
async fn unpair(State(ctx): State<Ctx>, headers: HeaderMap) -> Response {
    let Some(p) = Bridge::pairing_for(&ctx.core, &headers) else {
        return error(StatusCode::UNAUTHORIZED, "not-paired");
    };
    if let Err(e) = Bridge::revoke(&ctx.core, &p.id) {
        return error(StatusCode::INTERNAL_SERVER_ERROR, &e);
    }
    ctx.core.log.info("A browser extension was unpaired");
    Json(serde_json::json!({ "unpaired": true })).into_response()
}

async fn list_jobs(State(ctx): State<Ctx>, headers: HeaderMap) -> Response {
    if !Bridge::check_token(&ctx.core, &headers) {
        return error(StatusCode::UNAUTHORIZED, "not-paired");
    }
    Json(ctx.core.engine.summaries()).into_response()
}

async fn create_job(
    State(ctx): State<Ctx>,
    headers: HeaderMap,
    Json(request): Json<JobRequest>,
) -> Response {
    if !Bridge::check_token(&ctx.core, &headers) {
        return error(StatusCode::UNAUTHORIZED, "not-paired");
    }
    let (job_id, duplicate) = ctx.core.engine.create_checked(request);
    Json(serde_json::json!({ "jobId": job_id, "duplicate": duplicate })).into_response()
}

async fn job_action(
    State(ctx): State<Ctx>,
    headers: HeaderMap,
    UrlPath((id, action)): UrlPath<(String, String)>,
) -> Response {
    if !Bridge::check_token(&ctx.core, &headers) {
        return error(StatusCode::UNAUTHORIZED, "not-paired");
    }
    let action = match action.as_str() {
        "pause" => JobAction::Pause,
        "resume" => JobAction::Resume,
        "cancel" => JobAction::Cancel,
        "retry" => JobAction::Retry,
        "redownload" => JobAction::Redownload,
        _ => return error(StatusCode::BAD_REQUEST, "unknown-action"),
    };
    ctx.core.engine.act(&id, action);
    StatusCode::NO_CONTENT.into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchBody {
    service: String,
    creator_id: String,
    #[serde(default)]
    name: String,
}

async fn watch(
    State(ctx): State<Ctx>,
    headers: HeaderMap,
    Json(body): Json<WatchBody>,
) -> Response {
    if !Bridge::check_token(&ctx.core, &headers) {
        return error(StatusCode::UNAUTHORIZED, "not-paired");
    }
    let name = if body.name.is_empty() {
        body.creator_id.clone()
    } else {
        body.name
    };
    match ctx.core.watcher.add(&body.service, &body.creator_id, &name) {
        Ok(added) => Json(serde_json::json!({ "added": added })).into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_extensions_on_loopback() {
        assert!(is_extension_origin("chrome-extension://abcdef"));
        assert!(is_extension_origin("moz-extension://1234"));
        assert!(!is_extension_origin("https://evil.example"));
        assert!(!is_extension_origin("null"));
        assert!(is_loopback_host("127.0.0.1:49517"));
        assert!(is_loopback_host("localhost:49517"));
        assert!(!is_loopback_host("evil.example:49517"));
        assert_ne!(hash("a"), hash("b"));
    }
}
