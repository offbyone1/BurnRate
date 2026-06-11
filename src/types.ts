export interface WindowUsage {
  utilization: number;
  resets_at: string | null;
}

export interface ExtraUsage {
  is_enabled: boolean;
  monthly_limit: number | null;
  used_credits: number | null;
  utilization: number | null;
  currency?: string | null;
}

export interface ClaudeUsageResponse {
  five_hour: WindowUsage | null;
  seven_day: WindowUsage | null;
  extra_usage: ExtraUsage | null;
}

/** Settings input shape for save_settings. saved_at is set server-side. */
export interface Settings {
  session_key: string | null;
  org_id: string | null;
}

export interface SettingsDisplay {
  has_session_key: boolean;
  session_key: string | null;
  org_id: string | null;
  saved_at: number | null;
}

export type ViewState = "compact" | "expanded" | "settings";

/** Mirrors `api_types::CodexWindowUsage`. utilization is 0-100. */
export interface CodexWindowUsage {
  utilization: number;
  windowMinutes: number;
  resetsAt: string | null;
}

/** Mirrors `api_types::CodexUsage`. planType is null for API-key auth. */
export interface CodexUsage {
  planType: string | null;
  primary: CodexWindowUsage | null;
  secondary: CodexWindowUsage | null;
  snapshotAt: string;
}


