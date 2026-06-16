use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use tauri::{AppHandle, State};
use tauri_plugin_store::StoreExt;

use crate::api_types::{ClaudeUsageResponse, CodexUsage, CodexWindowUsage, ExtraUsage, Settings, SettingsDisplay, WindowUsage};

const USER_AGENT: &str = concat!("BurnRate-Widget/", env!("CARGO_PKG_VERSION"));

// Suppresses the console window Windows flashes when a GUI process spawns a
// console-subsystem child (we spawn `cmd`/`npx`/`where` for the TokenBBQ link).
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn is_valid_uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

fn is_valid_session_key(s: &str) -> bool {
    !s.is_empty()
        && s.len() < 1024
        && s.bytes().all(|b| b.is_ascii_graphic())
        && !s.contains('\r')
        && !s.contains('\n')
}

const KEYRING_SERVICE: &str = "com.offbyone1.burnrate";
const KEYRING_USER: &str = "session_key";

const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CODEX_TTL: Duration = Duration::from_secs(60);
static CODEX_CACHE: Mutex<Option<(Instant, Option<CodexUsage>)>> = Mutex::new(None);

fn keyring_get() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Keyring error: {}", e))?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read from credential store: {}", e)),
    }
}

fn keyring_set(key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Keyring error: {}", e))?;
    entry
        .set_password(key)
        .map_err(|e| format!("Failed to save to credential store: {}", e))
}

async fn keyring_get_async() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(keyring_get)
        .await
        .map_err(|e| format!("Task error: {}", e))?
}

async fn keyring_set_async(key: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || keyring_set(&key))
        .await
        .map_err(|e| format!("Task error: {}", e))?
}

// === OAuth path (primary, since 0.6.x) ===
//
// Reads the OAuth access-token Claude Code stores in ~/.claude/.credentials.json,
// POSTs `max_tokens=0` to api.anthropic.com/v1/messages, and parses the
// `anthropic-ratelimit-unified-*` response headers. This eliminates the manual
// sessionKey-paste flow entirely. Cost per refresh: ~8 input tokens against the
// user's 5h plan window (Pro/Max), throttled to one call per 60s — sub-promille
// drift on the displayed numbers.
//
// Surfaces it relies on:
//   - .credentials.json schema (claudeAiOauth.accessToken) — undocumented
//   - anthropic-ratelimit-unified-* response headers — undocumented
// If either changes, the legacy sessionKey path below can be re-enabled.

const OAUTH_TTL: Duration = Duration::from_secs(60);

struct CachedUsage {
    fetched_at: Instant,
    response: ClaudeUsageResponse,
}

static OAUTH_CACHE: Mutex<Option<CachedUsage>> = Mutex::new(None);

fn credentials_path() -> Option<PathBuf> {
    // Honor CLAUDE_CONFIG_DIR like Claude Code's own config resolution does, so users
    // with non-default install layouts still work.
    if let Ok(custom) = std::env::var("CLAUDE_CONFIG_DIR") {
        return Some(PathBuf::from(custom).join(".credentials.json"));
    }
    let home = std::env::var("HOME")
        .ok()
        .or_else(|| std::env::var("USERPROFILE").ok())?;
    Some(PathBuf::from(home).join(".claude").join(".credentials.json"))
}

#[derive(serde::Deserialize)]
struct CredentialsFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: Option<ClaudeAiOauth>,
}

#[derive(serde::Deserialize)]
struct ClaudeAiOauth {
    #[serde(rename = "accessToken")]
    access_token: String,
}

async fn read_oauth_token() -> Result<String, String> {
    let path = credentials_path()
        .ok_or("Could not resolve Claude credentials path (no HOME/USERPROFILE).")?;
    let contents = tokio::fs::read_to_string(&path)
        .await
        .map_err(|e| {
            format!(
                "Could not read {}: {}. Run `claude auth login` to create it.",
                path.display(),
                e
            )
        })?;
    let parsed: CredentialsFile = serde_json::from_str(&contents)
        .map_err(|e| format!("Could not parse credentials file: {}", e))?;
    parsed
        .claude_ai_oauth
        .ok_or_else(|| "credentials.json is missing claudeAiOauth section.".to_string())
        .map(|o| o.access_token)
}

fn header_str(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers.get(name)?.to_str().ok().map(String::from)
}

fn parse_utilization_pct(raw: Option<String>) -> f64 {
    raw.and_then(|v| v.parse::<f64>().ok())
        .map(|v| (v * 100.0).clamp(0.0, 100.0))
        .unwrap_or(0.0)
}

fn parse_unix_to_iso(raw: Option<String>) -> Option<String> {
    let secs: i64 = raw?.parse().ok()?;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0).map(|dt| dt.to_rfc3339())
}

async fn fetch_via_oauth_headers(
    client: &reqwest::Client,
    token: &str,
) -> Result<ClaudeUsageResponse, String> {
    // Cheapest legal /v1/messages call: max_tokens=0 with a 1-char prompt.
    // Empirically returns 200 with the full anthropic-ratelimit-unified-*
    // header set; validation errors (400/404) and count_tokens do NOT include
    // these headers, so we have to actually make a successful call.
    let body = serde_json::json!({
        "model": "claude-haiku-4-5",
        "max_tokens": 0,
        "messages": [{"role": "user", "content": "x"}]
    });

    let resp = client
        .post("https://api.anthropic.com/v1/messages")
        .header("Authorization", format!("Bearer {}", token))
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .header("User-Agent", USER_AGENT)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let status = resp.status();
    let headers = resp.headers().clone();
    // Drain body without parsing — the rate-limit data lives in headers, the
    // body itself is just the empty assistant response.
    let _ = resp.bytes().await;

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(
            "OAuth token rejected. Run `claude auth login` to refresh credentials.".to_string(),
        );
    }
    if !status.is_success() {
        return Err(format!("Anthropic API error: HTTP {}", status.as_u16()));
    }

    let five_h_util = header_str(&headers, "anthropic-ratelimit-unified-5h-utilization");
    let five_h_reset = header_str(&headers, "anthropic-ratelimit-unified-5h-reset");
    let seven_d_util = header_str(&headers, "anthropic-ratelimit-unified-7d-utilization");
    let seven_d_reset = header_str(&headers, "anthropic-ratelimit-unified-7d-reset");
    let overage_status = header_str(&headers, "anthropic-ratelimit-unified-overage-status");
    let overage_util = header_str(&headers, "anthropic-ratelimit-unified-overage-utilization");

    if five_h_util.is_none() && seven_d_util.is_none() {
        return Err(
            "Anthropic response missing unified rate-limit headers. The undocumented header schema may have changed."
                .to_string(),
        );
    }

    let five_hour = five_h_util.as_ref().map(|_| WindowUsage {
        utilization: parse_utilization_pct(five_h_util.clone()),
        resets_at: parse_unix_to_iso(five_h_reset),
    });
    let seven_day = seven_d_util.as_ref().map(|_| WindowUsage {
        utilization: parse_utilization_pct(seven_d_util.clone()),
        resets_at: parse_unix_to_iso(seven_d_reset),
    });

    // Overage = the existing extra_usage shape. Unified headers don't expose
    // monthly_limit / used_credits / currency, so those stay None and the UI
    // falls back to the unlimited-style display (utilization bar without a
    // "$X / $Y" meta line). is_enabled tracks the overage-status header:
    // "allowed" → enabled, anything else (or absent) → not enabled.
    let extra_usage = overage_status.as_deref().filter(|s| !s.is_empty()).map(|s| ExtraUsage {
        is_enabled: s == "allowed",
        monthly_limit: None,
        used_credits: None,
        utilization: Some(parse_utilization_pct(overage_util)),
        currency: None,
    });

    Ok(ClaudeUsageResponse {
        five_hour,
        seven_day,
        extra_usage,
    })
}

#[tauri::command]
pub async fn fetch_usage(client: State<'_, reqwest::Client>) -> Result<ClaudeUsageResponse, String> {
    // 60s cache — keeps quota consumption sub-promille even if the frontend
    // polls every few seconds. Cache hit path is a single Mutex lock + clone.
    if let Ok(guard) = OAUTH_CACHE.lock() {
        if let Some(c) = guard.as_ref() {
            if c.fetched_at.elapsed() < OAUTH_TTL {
                return Ok(c.response.clone());
            }
        }
    }

    let token = read_oauth_token().await?;
    let response = fetch_via_oauth_headers(&client, &token).await?;

    if let Ok(mut guard) = OAUTH_CACHE.lock() {
        *guard = Some(CachedUsage {
            fetched_at: Instant::now(),
            response: response.clone(),
        });
    }

    Ok(response)
}

// === LEGACY sessionKey path (retained, commented out, as fallback) =========
//
// The pre-0.6 implementation called claude.ai/api/organizations/{org_id}/usage
// using the user's manually-pasted sessionKey cookie + auto-detected org UUID.
// It produced richer ExtraUsage data (monthly_limit, used_credits, currency)
// that the unified-headers path cannot expose, but required a manual paste
// from browser devtools and silently broke whenever Anthropic rotated the
// cookie name or hardened CSRF.
//
// To re-enable (e.g. if Anthropic removes the unified-* headers or moves
// .credentials.json into the OS keystore):
//   1. Uncomment the `claude_get` helper and the body of `fetch_usage_via_session_key` below.
//   2. Replace the body of `fetch_usage` above with:
//          fetch_usage_via_session_key(app, client).await
//      (and re-add `app: AppHandle` to its signature).
//   3. Add `auto_detect_org` back to lib.rs's invoke_handler list if it was removed.
//
// /*
// async fn claude_get(client: &reqwest::Client, url: &str, session_key: &str) -> Result<reqwest::Response, String> {
//     let resp = client
//         .get(url)
//         .header("Cookie", format!("sessionKey={}", session_key))
//         .header("Content-Type", "application/json")
//         .header("User-Agent", USER_AGENT)
//         .send()
//         .await
//         .map_err(|e| format!("Network error: {}", e))?;
//
//     let status = resp.status();
//     if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
//         return Err("Session expired. Update your session key in Settings.".to_string());
//     }
//     if !status.is_success() {
//         return Err(format!("API error: HTTP {}", status.as_u16()));
//     }
//
//     Ok(resp)
// }
//
// async fn fetch_usage_via_session_key(app: AppHandle, client: State<'_, reqwest::Client>) -> Result<ClaudeUsageResponse, String> {
//     let session_key = keyring_get_async()
//         .await?
//         .ok_or("No session key configured.")?;
//
//     let store = app.store("settings.json").map_err(|e| e.to_string())?;
//     let org_id = store
//         .get("org_id")
//         .and_then(|v| v.as_str().map(String::from))
//         .ok_or("No organization ID configured.")?;
//
//     if !is_valid_uuid(&org_id) {
//         return Err("Invalid organization ID format.".to_string());
//     }
//
//     let url = format!("https://claude.ai/api/organizations/{}/usage", org_id);
//
//     claude_get(&client, &url, &session_key)
//         .await?
//         .json::<ClaudeUsageResponse>()
//         .await
//         .map_err(|e| format!("Parse error: {}", e))
// }
// */

#[tauri::command]
pub async fn save_settings(app: AppHandle, settings: Settings) -> Result<(), String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    if let Some(ref key) = settings.session_key {
        if !is_valid_session_key(key) {
            return Err("Invalid session key format.".to_string());
        }
        keyring_set_async(key.clone()).await?;
        store.set("saved_at", serde_json::json!(now));
    }
    if let Some(ref oid) = settings.org_id {
        if !is_valid_uuid(oid) {
            return Err("Invalid organization ID format.".to_string());
        }
        store.set("org_id", serde_json::json!(oid));
    }

    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn load_settings(app: AppHandle) -> Result<SettingsDisplay, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    // Read from keyring, with migration from legacy plaintext store
    let mut session_key = keyring_get_async().await?;

    if session_key.is_none() {
        // Migration: check both legacy store key names
        let store_key = store
            .get("session_key")
            .and_then(|v| v.as_str().map(String::from))
            .or_else(|| {
                store
                    .get("claude_session_key")
                    .and_then(|v| v.as_str().map(String::from))
            });

        if let Some(key) = store_key {
            keyring_set_async(key.clone()).await?;
            store.delete("session_key");
            store.delete("claude_session_key");
            store.save().map_err(|e| e.to_string())?;
            session_key = Some(key);
        }
    }

    Ok(SettingsDisplay {
        has_session_key: session_key.is_some(),
        org_id: store.get("org_id").and_then(|v| v.as_str().map(String::from)),
        saved_at: store.get("saved_at").and_then(|v| v.as_u64()),
    })
}

// auto_detect_org is part of the legacy sessionKey path. With the OAuth
// fetch_usage primary, the org-id arrives via `anthropic-organization-id`
// response header on every successful /v1/messages call, so detection is
// implicit and this RPC is no longer needed. Kept commented as fallback.
//
// /*
// #[tauri::command]
// pub async fn auto_detect_org(client: State<'_, reqwest::Client>, session_key: String) -> Result<String, String> {
//     if !is_valid_session_key(&session_key) {
//         return Err("Invalid session key format.".to_string());
//     }
//
//     let resp = claude_get(&client, "https://claude.ai/api/organizations", &session_key).await?;
//
//     let orgs: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;
//
//     orgs.first()
//         .and_then(|o| o["uuid"].as_str().map(String::from))
//         .ok_or("No organizations found".to_string())
// }
// */

const TOKENBBQ_FALLBACK_URL: &str = "https://github.com/offbyone1/tokenbbq";
// Must match TokenBBQ's default port (src/index.ts). 3005, not 3000: port 3000
// is constantly taken by local dev servers, which would push TokenBBQ onto a
// fallback port we can't predict and the "already serving?" check would miss.
const TOKENBBQ_PORT: u16 = 3005;
const TOKENBBQ_READY_TIMEOUT: Duration = Duration::from_secs(120);

/// Open TokenBBQ's dashboard in the browser. BurnRate shows only percentages;
/// TokenBBQ is the companion that shows exact token counts and costs.
///
/// Driven by the loopback PORT, not stdout — the published TokenBBQ CLI prints
/// nothing parseable under `--no-open`, and its output format differs between
/// versions. TokenBBQ serves on a fixed port (3005) and refuses to start a
/// second instance on a taken port, so the port itself is the source of truth:
///   * already serving on :3005  -> just open the browser (no duplicate, instant)
///   * nothing there             -> spawn `npx tokenbbq@latest`, wait until the
///                                  port answers, THEN open the browser
/// `--no-open` lets us own the browser-open so the frontend's await resolves
/// exactly when the dashboard is actually reachable.
#[tauri::command]
pub async fn open_tokenbbq() -> Result<(), String> {
    tokio::task::spawn_blocking(|| -> Result<(), String> {
        let url = format!("http://127.0.0.1:{TOKENBBQ_PORT}");

        // Already up (this or a previous session)? Just bring it to the front.
        if tokenbbq_dashboard_up(TOKENBBQ_PORT) {
            return open_url_detached(&url);
        }

        // Not up — we need npx. If Node isn't installed, open the project page
        // instead of dead-ending.
        if !npx_available() {
            return open_url_detached(TOKENBBQ_FALLBACK_URL);
        }

        // Launch the dashboard and wait until the port actually answers.
        let mut child = spawn_tokenbbq_detached(TOKENBBQ_PORT)?;
        let deadline = Instant::now() + TOKENBBQ_READY_TIMEOUT;
        loop {
            if tokenbbq_dashboard_up(TOKENBBQ_PORT) {
                return open_url_detached(&url);
            }
            // npx/node exited before the server came up (e.g. the port was held
            // by something else) — fail fast instead of waiting out the timeout.
            if let Ok(Some(status)) = child.try_wait() {
                return Err(format!(
                    "TokenBBQ exited before its dashboard was ready (code {:?})",
                    status.code()
                ));
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                return Err("TokenBBQ did not start in time".to_string());
            }
            std::thread::sleep(Duration::from_millis(300));
        }
    })
    .await
    .map_err(|e| format!("Task error: {}", e))?
}

fn npx_available() -> bool {
    // `where`/`which` returns success only if npx is on PATH.
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "where", "npx"]);
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = std::process::Command::new("which");
        c.arg("npx");
        c
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    matches!(cmd.status(), Ok(s) if s.success())
}

/// Spawn `npx tokenbbq@latest --no-open --port=<port>` detached. The dashboard
/// keeps running after the widget closes (it outlives the widget, by design).
/// All stdio is silenced; readiness is detected via the port, not the output.
fn spawn_tokenbbq_detached(port: u16) -> Result<std::process::Child, String> {
    let port_arg = format!("--port={port}");
    // npx on Windows is a .cmd shim — must go through `cmd /C`; a bare
    // Command::new("npx") fails with "program not found".
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "npx", "-y", "tokenbbq@latest", "--no-open", &port_arg]);
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = std::process::Command::new("npx");
        c.args(["-y", "tokenbbq@latest", "--no-open", &port_arg]);
        c
    };
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn()
        .map_err(|e| format!("Failed to launch TokenBBQ via npx: {}", e))
}

/// Whether a TokenBBQ dashboard is answering on `port`. Sends a tiny HTTP GET
/// and checks for the "TokenBBQ" marker so an unrelated service squatting on
/// the port isn't mistaken for ours.
fn tokenbbq_dashboard_up(port: u16) -> bool {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    let Ok(addr) = format!("127.0.0.1:{port}").parse::<SocketAddr>() else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, Duration::from_millis(400)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
    let req = "GET / HTTP/1.0\r\nHost: localhost\r\nConnection: close\r\n\r\n";
    if stream.write_all(req.as_bytes()).is_err() {
        return false;
    }
    let mut buf = [0u8; 4096];
    let mut total = 0;
    while total < buf.len() {
        match stream.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf[..total]).contains("TokenBBQ")
}

fn open_url_detached(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        // Empty "" is the window title arg `start` expects before the URL.
        c.args(["/C", "start", "", url]);
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(not(windows))]
    cmd.arg(url);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open URL: {}", e))
}

fn codex_dir() -> Option<PathBuf> {
    // Mirror getCodexDir() from codex.ts: require a sessions/ subdir to exist.
    if let Ok(env_path) = std::env::var("CODEX_HOME") {
        let p = PathBuf::from(env_path);
        if p.join("sessions").is_dir() { return Some(p); }
    }
    let home = std::env::var("HOME").ok().or_else(|| std::env::var("USERPROFILE").ok())?;
    let p = PathBuf::from(home).join(".codex");
    if p.join("sessions").is_dir() { Some(p) } else { None }
}

async fn read_codex_auth(codex_dir: &Path) -> Option<(String, Option<String>)> {
    let raw = tokio::fs::read_to_string(codex_dir.join("auth.json")).await.ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&raw).ok()?;
    // Token data may live under `.tokens` or at the top level (mirror TS).
    let tokens = parsed.get("tokens").filter(|v| v.is_object()).unwrap_or(&parsed);
    let access = tokens.get("access_token")?.as_str()?.trim().to_string();
    if access.is_empty() { return None; }
    let account_id = tokens
        .get("account_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    Some((access, account_id))
}

fn num_from_value(v: Option<&serde_json::Value>) -> Option<f64> {
    match v {
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn ms_to_iso(ms: f64) -> String {
    let secs = (ms / 1000.0).floor() as i64;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// Port of parseRateLimitWindow (codex.ts). `now_ms` is injected for testability.
fn parse_rate_limit_window(raw: Option<&serde_json::Value>, now_ms: f64) -> Option<CodexWindowUsage> {
    let r = raw?.as_object()?;
    let used = num_from_value(r.get("used_percent"))?;
    let window_min = num_from_value(r.get("window_minutes"))
        .or_else(|| num_from_value(r.get("limit_window_seconds")).map(|s| s / 60.0))?;
    let resets_unix = num_from_value(r.get("resets_at").or_else(|| r.get("reset_at")));
    let mut resets_at_ms = resets_unix.map(|s| s * 1000.0);
    let mut used_eff = used;
    if let Some(rms) = resets_at_ms {
        if rms < now_ms {
            let window_ms = window_min * 60.0 * 1000.0;
            let elapsed_windows = ((now_ms - rms) / window_ms).floor() + 1.0;
            resets_at_ms = Some(rms + elapsed_windows * window_ms);
            used_eff = 0.0;
        }
    }
    Some(CodexWindowUsage {
        utilization: used_eff,
        window_minutes: window_min.round().max(0.0) as u32,
        resets_at: resets_at_ms.map(ms_to_iso),
    })
}

/// Port of parseCodexRateLimitsFromWhamUsage (codex.ts).
fn parse_codex_rate_limits(body: &serde_json::Value, snapshot_at: String, now_ms: f64) -> Option<CodexUsage> {
    let usage = body.as_object()?;
    let rate_limit = usage.get("rate_limit")?.as_object()?;
    let plan_type = usage.get("plan_type").and_then(|v| v.as_str()).map(String::from);
    let primary = parse_rate_limit_window(rate_limit.get("primary_window"), now_ms);
    let secondary = parse_rate_limit_window(rate_limit.get("secondary_window"), now_ms);
    if primary.is_none() && secondary.is_none() { return None; }
    Some(CodexUsage { plan_type, primary, secondary, snapshot_at })
}

/// Read Codex account rate limits live from chatgpt.com/backend-api/wham/usage,
/// using the access token in ~/.codex/auth.json. Returns Ok(None) when Codex
/// is not configured (no sessions dir / no auth), mirroring the TS behavior of
/// keeping the pill's Codex tiles empty rather than erroring.
#[tauri::command]
pub async fn fetch_codex_usage(client: State<'_, reqwest::Client>) -> Result<Option<CodexUsage>, String> {
    if let Ok(guard) = CODEX_CACHE.lock() {
        if let Some((at, resp)) = guard.as_ref() {
            if at.elapsed() < CODEX_TTL { return Ok(resp.clone()); }
        }
    }

    let Some(dir) = codex_dir() else { return Ok(None); };
    let Some((token, account_id)) = read_codex_auth(&dir).await else { return Ok(None); };

    let mut req = client
        .get(CODEX_USAGE_URL)
        .header("Authorization", format!("Bearer {}", token))
        .header("OAI-Language", "en")
        .header("originator", "Codex Desktop")
        .header("User-Agent", USER_AGENT);
    if let Some(acc) = account_id {
        req = req.header("ChatGPT-Account-ID", acc);
    }

    let resp = req.send().await.map_err(|e| format!("Network error: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("Codex usage API error: HTTP {}", resp.status().as_u16()));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| format!("Parse error: {}", e))?;

    let snapshot_at = chrono::Utc::now().to_rfc3339();
    let now_ms = chrono::Utc::now().timestamp_millis() as f64;
    let snapshot = parse_codex_rate_limits(&body, snapshot_at, now_ms);

    if let Ok(mut guard) = CODEX_CACHE.lock() {
        *guard = Some((Instant::now(), snapshot.clone()));
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Fixed "now" so tests are deterministic (no wall-clock).
    const NOW_MS: f64 = 1_700_000_000_000.0; // 2023-11-14T22:13:20Z

    #[test]
    fn window_future_reset_keeps_used() {
        let raw = json!({ "used_percent": 42.5, "window_minutes": 300,
            "resets_at": (NOW_MS / 1000.0) + 600.0 });
        let w = parse_rate_limit_window(Some(&raw), NOW_MS).unwrap();
        assert_eq!(w.utilization, 42.5);
        assert_eq!(w.window_minutes, 300);
        assert!(w.resets_at.is_some());
    }

    #[test]
    fn window_expired_reset_zeroes_and_advances() {
        // reset 1 hour in the past, 60-min window → used resets to 0, resets_at moves to the future.
        let raw = json!({ "used_percent": 88.0, "window_minutes": 60,
            "resets_at": (NOW_MS / 1000.0) - 3600.0 });
        let w = parse_rate_limit_window(Some(&raw), NOW_MS).unwrap();
        assert_eq!(w.utilization, 0.0);
        assert!(w.resets_at.is_some());
    }

    #[test]
    fn window_accepts_limit_window_seconds_and_reset_at_alias() {
        let raw = json!({ "used_percent": "10", "limit_window_seconds": 18000,
            "reset_at": (NOW_MS / 1000.0) + 60.0 });
        let w = parse_rate_limit_window(Some(&raw), NOW_MS).unwrap();
        assert_eq!(w.utilization, 10.0);
        assert_eq!(w.window_minutes, 300); // 18000s / 60
    }

    #[test]
    fn window_missing_fields_returns_none() {
        assert!(parse_rate_limit_window(Some(&json!({ "window_minutes": 60 })), NOW_MS).is_none());
        assert!(parse_rate_limit_window(Some(&json!({ "used_percent": 5 })), NOW_MS).is_none());
        assert!(parse_rate_limit_window(None, NOW_MS).is_none());
    }

    #[test]
    fn snapshot_parses_primary_and_secondary() {
        let body = json!({ "plan_type": "plus", "rate_limit": {
            "primary_window": { "used_percent": 20, "window_minutes": 300, "resets_at": (NOW_MS/1000.0)+10.0 },
            "secondary_window": { "used_percent": 5, "window_minutes": 10080, "resets_at": (NOW_MS/1000.0)+10.0 }
        }});
        let s = parse_codex_rate_limits(&body, "2023-11-14T22:13:20Z".into(), NOW_MS).unwrap();
        assert_eq!(s.plan_type.as_deref(), Some("plus"));
        assert!(s.primary.is_some() && s.secondary.is_some());
    }

    #[test]
    fn snapshot_null_plan_type_still_returns_when_windows_present() {
        let body = json!({ "rate_limit": {
            "primary_window": { "used_percent": 1, "window_minutes": 300, "resets_at": (NOW_MS/1000.0)+10.0 }
        }});
        let s = parse_codex_rate_limits(&body, "x".into(), NOW_MS).unwrap();
        assert!(s.plan_type.is_none());
        assert!(s.primary.is_some());
    }

    #[test]
    fn snapshot_without_rate_limit_is_none() {
        assert!(parse_codex_rate_limits(&json!({ "plan_type": "plus" }), "x".into(), NOW_MS).is_none());
    }
}
