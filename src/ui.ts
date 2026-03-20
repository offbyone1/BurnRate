import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import type { ClaudeUsageResponse, ViewState } from "./types";

const COMPACT_SIZE = { width: 270, height: 64 };
const EXPANDED_SIZE = { width: 340, height: 520 };

type ColorTier = "green" | "orange" | "red";

function colorTier(pct: number): ColorTier {
  if (pct < 50) return "green";
  if (pct < 80) return "orange";
  return "red";
}

export function utilizationColor(pct: number): string {
  return `var(--${colorTier(pct)})`;
}

function formatTimeUntil(isoString: string | null): string {
  if (!isoString) return "";
  const diffMs = new Date(isoString).getTime() - Date.now();
  if (diffMs <= 0) return "Resetting...";
  const hours = Math.floor(diffMs / 3600000);
  const minutes = Math.floor((diffMs % 3600000) / 60000);
  if (hours >= 24) {
    return `Resets in ${Math.floor(hours / 24)}d ${hours % 24}h`;
  }
  return `Resets in ${hours}h ${minutes}m`;
}

function formatResetDate(isoString: string | null): string {
  if (!isoString) return "";
  const diffMs = new Date(isoString).getTime() - Date.now();
  if (diffMs < 86400000) return formatTimeUntil(isoString);
  return `Resets ${new Date(isoString).toLocaleDateString("en-US", { month: "short", day: "numeric" })}`;
}

const clockSvg = `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M1.5 8a6.5 6.5 0 1 1 1 3.5"/><path d="M1 5v3.5H4.5"/></svg>`;

function usageRowHtml(
  name: string,
  usage: { utilization: number; resets_at: string | null } | null,
): string {
  if (!usage) return "";
  const pct = usage.utilization;
  const tier = colorTier(pct);
  const color = `var(--${tier})`;
  const glow = `var(--${tier}-glow)`;
  const resetText = name === "5-Hour Window"
    ? formatTimeUntil(usage.resets_at)
    : formatResetDate(usage.resets_at);

  return `
    <div class="usage-row">
      <div class="usage-row-header">
        <span class="usage-row-name"><span class="dot" style="background:${color};box-shadow:0 0 4px ${glow}"></span>${name}</span>
        <span class="usage-row-value" style="color:${color}">${Math.round(pct)}%</span>
      </div>
      <div class="progress-track"><div class="progress-fill ${tier}" style="width:${pct}%"></div></div>
      ${resetText ? `<div class="usage-row-meta"><span class="usage-row-reset">${clockSvg}${resetText}</span></div>` : ""}
    </div>`;
}

export function renderCompact(usage: ClaudeUsageResponse): void {
  const fiveHour = document.getElementById("five-hour-compact")!;
  const sevenDay = document.getElementById("seven-day-compact")!;

  const fhPct = usage.five_hour?.utilization ?? 0;
  const sdPct = usage.seven_day?.utilization ?? 0;

  fiveHour.textContent = `${Math.round(fhPct)}%`;
  fiveHour.style.color = utilizationColor(fhPct);
  sevenDay.textContent = `${Math.round(sdPct)}%`;
  sevenDay.style.color = utilizationColor(sdPct);
}

export function renderExpanded(usage: ClaudeUsageResponse): void {
  const container = document.getElementById("usage-bars")!;

  let html = `<div class="usage-section-label">Claude</div>`;
  html += usageRowHtml("5-Hour Window", usage.five_hour);
  html += usageRowHtml("7-Day Window", usage.seven_day);

  if (usage.extra_usage) {
    const ex = usage.extra_usage;
    const tier = colorTier(ex.utilization);
    html += `
      <div class="credits-row" style="margin-top:6px">
        <div class="credits-header">
          <span class="credits-label"><span class="icon">&#x26A1;</span>Extra Usage</span>
          <span class="credits-amount">
            <span class="used">&euro;${(ex.used_credits / 100).toFixed(2)}</span><span class="sep">/</span><span class="total">&euro;${(ex.monthly_limit / 100).toFixed(0)}</span>
          </span>
        </div>
        <div class="progress-track" style="margin-top:8px"><div class="progress-fill ${tier}" style="width:${ex.utilization}%"></div></div>
        <div class="credits-pct">${ex.utilization.toFixed(1)}%</div>
      </div>`;
  }

  container.innerHTML = html;
}

function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

export function renderError(message: string): void {
  document.getElementById("five-hour-compact")!.textContent = "—";
  document.getElementById("seven-day-compact")!.textContent = "err";

  document.getElementById("usage-bars")!.innerHTML = `
    <div class="error-banner">
      <span class="error-icon">&#x26A0;</span>
      <span>${escapeHtml(message)}</span>
    </div>`;
}

export async function setViewState(state: ViewState): Promise<void> {
  const pill = document.getElementById("compact-view")!;
  const panel = document.getElementById("expanded-view")!;
  const settings = document.getElementById("settings-overlay")!;
  const win = getCurrentWindow();

  if (state === "compact") {
    settings.classList.remove("visible");
    panel.classList.remove("visible");
    pill.classList.remove("hidden-pill");
    await win.setSize(new LogicalSize(COMPACT_SIZE.width, COMPACT_SIZE.height));
  } else if (state === "expanded") {
    settings.classList.remove("visible");
    pill.classList.add("hidden-pill");
    await win.setSize(new LogicalSize(EXPANDED_SIZE.width, EXPANDED_SIZE.height));
    requestAnimationFrame(() => panel.classList.add("visible"));
  } else if (state === "settings") {
    settings.classList.add("visible");
  }
}
