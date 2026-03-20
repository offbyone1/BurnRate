# BurnRate

Always-on-top desktop widget showing your AI subscription usage at a glance.

![MIT License](https://img.shields.io/badge/license-MIT-blue)
![Windows](https://img.shields.io/badge/platform-Windows-blue)
![Tauri v2](https://img.shields.io/badge/Tauri-v2-orange)

## What it does

BurnRate sits on top of your windows as a compact pill, showing how much of your Claude subscription you've used. Click to expand for detailed breakdowns with progress bars and reset timers.

**Currently supported:** Claude.ai (Pro/Max subscription usage)

## Features

- Compact always-on-top pill with live usage percentages
- Expandable detail view with progress bars and reset countdowns
- 5-hour window, 7-day window, and extra credits tracking (€)
- Dark/light theme toggle
- Auto-refresh every 60 seconds (pauses when hidden)
- System tray with hide/show/refresh/quit
- Autostart with Windows
- Drag handle on compact pill, titlebar drag on expanded view
- Session key stays in the Rust backend (never exposed to the webview)
- Org ID auto-detected from API

## Install

Download `burnrate.exe` from the [latest release](https://github.com/offbyone1/BurnRate/releases/latest) and run it. No installer needed.

**Requirements:** Windows 10/11 with WebView2 (pre-installed on modern Windows)

## Setup

1. Run `burnrate.exe`
2. Open [claude.ai](https://claude.ai) in your browser
3. Press `F12` → **Application** → **Cookies** → copy the `sessionKey` value
4. Paste it into BurnRate's Settings
5. Org ID is auto-detected — click **Save**

That's it. BurnRate will refresh every 60 seconds.

## Build from source

### Prerequisites

- [Rust](https://rustup.rs/) (1.70+)
- [Node.js](https://nodejs.org/) (20+)
- Windows: Microsoft C++ Build Tools

### Steps

```bash
git clone https://github.com/offbyone1/BurnRate.git
cd BurnRate
npm install
npx tauri build
```

The binary will be at `src-tauri/target/release/burnrate.exe`.

### Development

```bash
npm install
npx tauri dev
```

## How it works

1. BurnRate calls `claude.ai/api/organizations/{org_id}/usage` using your session cookie
2. The response includes utilization percentages for different rate limit windows
3. Data refreshes every 60 seconds
4. All credentials are stored locally — no external servers, no telemetry

## License

[MIT](LICENSE) - offbyone1
