import { invoke } from "@tauri-apps/api/core";
import { isEnabled, enable, disable } from "@tauri-apps/plugin-autostart";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { ClaudeUsageResponse, Settings } from "./types";
import { renderCompact, renderExpanded, renderError, setViewState } from "./ui";

const SESSION_KEY_LIFETIME_MS = 28 * 24 * 60 * 60 * 1000;

let currentView: "compact" | "expanded" | "settings" = "compact";
let pollTimer: ReturnType<typeof setInterval> | null = null;

let lastUsageJson = "";

async function fetchUsage(): Promise<void> {
  try {
    const usage = await invoke<ClaudeUsageResponse>("fetch_usage");
    const json = JSON.stringify(usage);
    if (json === lastUsageJson) return; // no change, skip re-render
    lastUsageJson = json;
    renderCompact(usage);
    renderExpanded(usage);
  } catch (e) {
    renderError(String(e));
  }
}

function startPolling(): void {
  if (pollTimer) clearInterval(pollTimer);
  fetchUsage();
  pollTimer = setInterval(fetchUsage, 60_000);
}

function stopPolling(): void {
  if (pollTimer) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

async function init(): Promise<void> {
  const settings = await invoke<Settings>("load_settings");

  if (settings.session_key) {
    startPolling();
  } else {
    await expand();
    openSettings();
  }

  const win = getCurrentWindow();
  win.onCloseRequested(async (event) => {
    event.preventDefault();
    stopPolling();
    await win.hide();
  });

  listen("refresh-usage", () => fetchUsage());
  listen("resume-polling", () => startPolling());
  setupEventListeners();
  setupDragRegions();
}

async function expand(): Promise<void> {
  currentView = "expanded";
  await setViewState("expanded");
}

async function collapse(): Promise<void> {
  currentView = "compact";
  await setViewState("compact");
}

async function openSettings(): Promise<void> {
  currentView = "settings";
  setViewState("settings");

  try {
    const settings = await invoke<Settings>("load_settings");
    if (settings.session_key)
      (document.getElementById("session-key-input") as HTMLInputElement).value = settings.session_key;
    if (settings.org_id)
      (document.getElementById("org-id-input") as HTMLInputElement).value = settings.org_id;

    if (settings.saved_at) {
      const expiresAt = settings.saved_at * 1000 + SESSION_KEY_LIFETIME_MS;
      const daysLeft = Math.max(0, Math.ceil((expiresAt - Date.now()) / (24 * 60 * 60 * 1000)));
      const el = document.getElementById("import-status")!;
      if (daysLeft <= 5) {
        el.textContent = `Session key expires in ${daysLeft} day${daysLeft === 1 ? "" : "s"}.`;
        el.className = "import-status error";
      } else {
        el.textContent = `Session key valid for ~${daysLeft} days.`;
        el.className = "import-status success";
      }
    }
  } catch {}
}

function closeSettings(): void {
  currentView = "expanded";
  document.getElementById("settings-overlay")!.classList.remove("visible");
}

async function saveSettings(): Promise<void> {
  const sessionKey = (document.getElementById("session-key-input") as HTMLInputElement).value.trim() || null;
  let orgId = (document.getElementById("org-id-input") as HTMLInputElement).value.trim() || null;

  if (!sessionKey) return;

  const statusEl = document.getElementById("import-status")!;

  if (!orgId) {
    statusEl.textContent = "Detecting organization...";
    statusEl.className = "import-status loading";
    try {
      orgId = await invoke<string>("auto_detect_org", { sessionKey });
      (document.getElementById("org-id-input") as HTMLInputElement).value = orgId;
    } catch (e) {
      statusEl.textContent = "Could not detect Org ID: " + String(e);
      statusEl.className = "import-status error";
      return;
    }
  }

  try {
    await invoke("save_settings", {
      settings: { session_key: sessionKey, org_id: orgId, saved_at: null },
    });
    closeSettings();
    startPolling();
  } catch (e) {
    statusEl.textContent = String(e);
    statusEl.className = "import-status error";
  }
}

// --- Drag & Events ---

function setupDragRegions(): void {
  document.getElementById("pill-grip")!.addEventListener("mousedown", async () => {
    await getCurrentWindow().startDragging();
  });

  document.querySelectorAll(".titlebar").forEach((el) => {
    el.addEventListener("mousedown", async (e) => {
      if ((e.target as HTMLElement).closest("button")) return;
      await getCurrentWindow().startDragging();
    });
  });
}

function setupEventListeners(): void {
  document.getElementById("compact-view")!.addEventListener("click", expand);
  document.getElementById("btn-minimize")!.addEventListener("click", collapse);
  document.getElementById("btn-close")!.addEventListener("click", async () => {
    await getCurrentWindow().hide();
  });

  document.getElementById("btn-settings")!.addEventListener("click", async () => {
    if (currentView !== "expanded") await expand();
    openSettings();
  });

  document.getElementById("btn-refresh")!.addEventListener("click", () => {
    const btn = document.getElementById("btn-refresh")!;
    btn.classList.add("refreshing");
    fetchUsage().finally(() => setTimeout(() => btn.classList.remove("refreshing"), 500));
  });

  document.getElementById("btn-save-settings")!.addEventListener("click", saveSettings);
  document.getElementById("btn-cancel-settings")!.addEventListener("click", closeSettings);
  document.getElementById("btn-cancel-settings-2")!.addEventListener("click", closeSettings);

  document.querySelectorAll(".toggle-vis").forEach((btn) => {
    btn.addEventListener("click", () => {
      const input = btn.parentElement!.querySelector("input") as HTMLInputElement;
      input.type = input.type === "password" ? "text" : "password";
    });
  });

  // Autostart
  const autostartToggle = document.getElementById("autostart-toggle") as HTMLInputElement;
  isEnabled().then((enabled) => { autostartToggle.checked = enabled; });
  autostartToggle.addEventListener("change", async () => {
    if (autostartToggle.checked) {
      await enable();
    } else {
      await disable();
    }
  });

  // Theme
  const themeToggle = document.getElementById("theme-toggle") as HTMLInputElement;
  const savedTheme = localStorage.getItem("burnrate-theme") || "dark";
  applyTheme(savedTheme);
  themeToggle.checked = savedTheme === "light";
  themeToggle.addEventListener("change", () => {
    const theme = themeToggle.checked ? "light" : "dark";
    applyTheme(theme);
    localStorage.setItem("burnrate-theme", theme);
  });
}

function applyTheme(theme: string): void {
  document.documentElement.classList.remove("theme-dark", "theme-light");
  document.documentElement.classList.add(`theme-${theme}`);
  document.getElementById("theme-label-dark")!.classList.toggle("active", theme === "dark");
  document.getElementById("theme-label-light")!.classList.toggle("active", theme === "light");
}

document.addEventListener("DOMContentLoaded", init);
