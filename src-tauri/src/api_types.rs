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
    pub monthly_limit: f64,
    pub used_credits: f64,
    pub utilization: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub session_key: Option<String>,
    pub org_id: Option<String>,
    pub saved_at: Option<u64>,
}

/// Settings returned to the frontend. Session key is stored securely in the OS credential store.
#[derive(Debug, Clone, Serialize)]
pub struct SettingsDisplay {
    pub has_session_key: bool,
    pub session_key: Option<String>,
    pub org_id: Option<String>,
    pub saved_at: Option<u64>,
}
