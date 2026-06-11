use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeUsageResponse {
    pub five_hour: Option<WindowUsage>,
    pub seven_day: Option<WindowUsage>,
    pub extra_usage: Option<ExtraUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowUsage {
    pub utilization: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtraUsage {
    pub is_enabled: bool,
    pub monthly_limit: Option<f64>,
    pub used_credits: Option<f64>,
    pub utilization: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
}

/// Settings POSTed from the frontend. `saved_at` is set server-side on
/// every successful save, so we deliberately don't accept it as input.
#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    pub session_key: Option<String>,
    pub org_id: Option<String>,
}

/// Settings returned to the frontend. The plaintext session key never
/// leaves the OS credential store — only its presence as a flag, plus
/// non-secret metadata. Anything more would defeat the keyring migration.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsDisplay {
    pub has_session_key: bool,
    pub org_id: Option<String>,
    pub saved_at: Option<u64>,
}

/// Codex rate-limit window. camelCase JSON for the webview.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexWindowUsage {
    pub utilization: f64,
    pub window_minutes: u32,
    pub resets_at: Option<String>,
}

/// Live Codex rate-limit snapshot read from chatgpt.com/backend-api/wham/usage.
/// The pill renders these values when the Codex toggle is on. `plan_type` is
/// None for API-key auth (the UI then hides Codex).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUsage {
    pub plan_type: Option<String>,
    pub primary: Option<CodexWindowUsage>,
    pub secondary: Option<CodexWindowUsage>,
    pub snapshot_at: String,
}
