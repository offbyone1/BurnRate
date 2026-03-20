use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::api_types::{ClaudeUsageResponse, Settings};

const USER_AGENT: &str = concat!("BurnRate/", env!("CARGO_PKG_VERSION"));

async fn claude_get(url: &str, session_key: &str) -> Result<reqwest::Response, String> {
    let client = reqwest::Client::new();
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
pub async fn fetch_usage(app: AppHandle) -> Result<ClaudeUsageResponse, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    let session_key = store
        .get("session_key")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or("No session key configured.")?;

    let org_id = store
        .get("org_id")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or("No organization ID configured.")?;

    let url = format!("https://claude.ai/api/organizations/{}/usage", org_id);

    claude_get(&url, &session_key)
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
        store.set("session_key", serde_json::json!(key));
        store.set("saved_at", serde_json::json!(now));
    }
    if let Some(ref oid) = settings.org_id {
        store.set("org_id", serde_json::json!(oid));
    }

    store.save().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn load_settings(app: AppHandle) -> Result<Settings, String> {
    let store = app.store("settings.json").map_err(|e| e.to_string())?;

    Ok(Settings {
        session_key: store.get("session_key").and_then(|v| v.as_str().map(String::from)),
        org_id: store.get("org_id").and_then(|v| v.as_str().map(String::from)),
        saved_at: store.get("saved_at").and_then(|v| v.as_u64()),
    })
}

#[tauri::command]
pub async fn auto_detect_org(session_key: String) -> Result<String, String> {
    let resp = claude_get("https://claude.ai/api/organizations", &session_key).await?;

    let orgs: Vec<serde_json::Value> = resp.json().await.map_err(|e| e.to_string())?;

    orgs.first()
        .and_then(|o| o["uuid"].as_str().map(String::from))
        .ok_or("No organizations found".to_string())
}
