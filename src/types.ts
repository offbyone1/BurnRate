export interface WindowUsage {
  utilization: number;
  resets_at: string | null;
}

export interface ExtraUsage {
  is_enabled: boolean;
  monthly_limit: number;
  used_credits: number;
  utilization: number;
}

export interface ClaudeUsageResponse {
  five_hour: WindowUsage | null;
  seven_day: WindowUsage | null;
  extra_usage: ExtraUsage | null;
}

export interface Settings {
  session_key: string | null;
  org_id: string | null;
  saved_at: number | null;
}

export type ViewState = "compact" | "expanded" | "settings";
