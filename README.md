# BurnRate

Always-on-top desktop widget showing your AI subscription usage at a glance.

![MIT License](https://img.shields.io/badge/license-MIT-blue)

## What it does

BurnRate sits on top of your windows as a compact pill, showing how much of your Claude subscription you've used. Click to expand for detailed breakdowns with progress bars and reset timers.

**Currently supported:** Claude.ai (Pro/Max subscription usage)

## Features

- Compact always-on-top pill with live usage percentages
- Expandable detail view with progress bars and reset countdowns
- 5-hour window, 7-day window, per-model, and extra credits tracking
- Auto-refresh every 60 seconds
- System tray with hide/show/refresh/quit
- Dark theme with burnt orange accents
- Session key stays in the Rust backend (never exposed to the webview)

## Install

Download the latest release from [GitHub Releases](https://github.com/offbyone1/burnrate/releases).

## Build from source

### Prerequisites

- [Rust](https://rustup.rs/) (1.70+)
- [Node.js](https://nodejs.org/) (20+)
- Windows: Microsoft C++ Build Tools

### Steps

```bash
git clone https://github.com/offbyone1/burnrate.git
cd burnrate
npm install
npx tauri build
```

The installer will be in `src-tauri/target/release/bundle/`.

### Development

```bash
npm install
npx tauri dev
```

## Setup

1. Open [claude.ai](https://claude.ai) in your browser
2. Open DevTools (F12) → Application → Cookies
3. Copy the `sessionKey` value
4. For Org ID: check Network tab requests to `/organizations/{id}/`
5. Paste both into BurnRate's Settings

## How it works

1. BurnRate calls `claude.ai/api/organizations/{org_id}/usage` using your session cookie
2. The response includes utilization percentages for different rate limit windows
3. Data refreshes every 60 seconds
4. All credentials are stored locally — no external servers, no telemetry

## License

[MIT](LICENSE) - offbyone1
