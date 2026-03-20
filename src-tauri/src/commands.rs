use tauri::{AppHandle, State};
use tauri_plugin_store::StoreExt;

use crate::api_types::{ClaudeUsageResponse, Settings, SettingsDisplay};

const USER_AGENT: &str = concat!("BurnRate/", env!("CARGO_PKG_VERSION"));

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

fn mask_key(key: &str) -> String {
    if key.len() <= 12 {
        return "\u{2022}".repeat(8);
    }
    let prefix = &key[..12];
    format!("{}{}", prefix, "\u{2022}".repeat(8))
}

const KEYRING_SERVICE: &str = "com.offbyone1.burnrate";
const KEYRING_USER: &str = "session_key";

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

fn keyring_delete() -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| format!("Keyring error: {}", e))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Failed to delete from credential store: {}", e)),
    }
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

async fn keyring_delete_async() -> Result<(), String> {
    tokio::task::spawn_blocking(keyring_delete)
        .await
        .map_err(|e| format!("Task error: {}", e))?
}

async fn claude_get(client: &reqwest::Client, url: &str, session_key: &str) -> Result<reqwest::Response, String> {
    let resp = client
        .get(url)
        .header("Cookie", format!("sessionKey={}", session_key))
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err("Session expired. Update your session key in Settings.".to_string());
    }
    if !status.is_success() {
        return Err(format!("API error: HTTP {}", status.as_u16()));
    }

    Ok(resp)
}

#[tauri::command]
pub async fn fetch_usage(app: AppHandle, client: State<'_, reqwest::Client>) -> Result<ClaudeUsageResponse, String> {
    let session_key = keyring_get_async()
        .await?
        .ok_or("No session key configured.")?;

    let store = app.store("settings.json").map_err(|e| e.to_string())?;
    let org_id = store
        .get("org_id")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or("No organization ID configured.")?;

    if !is_valid_uuid(&org_id) {
        return Err("Invalid organization ID format.".to_string());
    }

    let url = format!("https://claude.ai/api/organizations/{}/usage", org_id);

    claude_get(&client, &url, &session_key)
        .await?
        .json::<ClaudeUsageResponse>()
        .await
        .map_err(|e| format!("Parse error: {}", e))
}

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
        session_key,
        org_id: store.get("org_id").and_then(|v| v.as_str().map(String::from)),
        saved_at: store.get("saved_at").and_then(|v| v.as_u64()),
    })
}

#[tauri::command]
pub async fn auto_detect_org(client: State<'_, reqwest::Client>, session_key: String) -> Result<String, String> {
    if !is_valid_session_key(&session_key) {
        return Err("Invalid session key format.".to_string());
    }

    let resp = claude_get(&client, "https://claude.ai/api/organizations", &session_key).await?;

    let orgs: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;

    orgs.first()
        .and_then(|o| o["uuid"].as_str().map(String::from))
        .ok_or("No organizations found".to_string())
}
