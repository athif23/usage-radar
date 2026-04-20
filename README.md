# Usage Radar

Usage Radar is a tray-first Windows desktop app for checking AI usage limits quickly.

It opens from the system tray and shows the current state of your Codex and GitHub Copilot usage in a compact popup, so you can answer a simple question fast:

> What can I safely use right now without guessing?

This project is built with Rust and [`iced`](https://github.com/iced-rs/iced). It is intentionally small, local-first, and honest about uncertainty.

- no backend
- no embedded browser app shell
- no billing dashboard sprawl
- no pretending partial data is exact

> [!IMPORTANT]
> Usage Radar is an unofficial utility. It is not affiliated with OpenAI or GitHub.

## Why it exists

AI usage limits are spread across too many places:

- browser dashboards
- account pages
- local CLI auth state
- provider-specific internal pages
- and sometimes nowhere obvious at all

That creates daily friction.

Usage Radar is meant to feel like a small Windows utility you can pop open many times a day for a fast, trustworthy answer.

## Current status

Usage Radar is currently an early but working Windows-first MVP.

Today it focuses on the first useful vertical slice:

- Codex
- GitHub Copilot
- compact tray popup UX
- honest refresh, stale, and unavailable states

## Features

- Tray-first app built with `iced::daemon`
- Compact popup panel anchored like a real Windows tray utility
- Bottom-right popup positioning above the taskbar on Windows
- Focus-aware tray click behavior:
  - hidden -> open
  - frontmost -> hide
  - behind other apps -> bring to front
- Real Codex usage fetch
- Real GitHub Copilot sign-in and usage fetch
- Local config and cache persistence
- Honest stale, unavailable, and partial-data handling

## Provider support

| Provider | Status | Source | Confidence |
| --- | --- | --- | --- |
| Codex | Working | `~/.codex/auth.json` or `CODEX_HOME/auth.json` + `https://chatgpt.com/backend-api/wham/usage` | Exact |
| GitHub Copilot | Working | GitHub device flow + `https://api.github.com/copilot_internal/user` | Partial |
| Claude Code | Planned | Not wired yet | — |
| Gemini CLI | Planned | Not wired yet | — |

### What "partial" means

GitHub Copilot can return usable quota percentages while still omitting some timing details, especially reset timing. Usage Radar shows that honestly instead of inventing precision.

## Why this repo may be useful if you're learning `iced`

Usage Radar is a small real app, not a demo widget gallery. If you are learning `iced`, this repo may be useful because it shows how to build a tray-first desktop utility around a compact popup UI.

Things this repo demonstrates:

- using `iced::daemon` instead of a normal always-visible app window
- integrating a tray icon with `tray-icon`
- keeping a reusable popup window hidden/shown instead of recreating it every click
- handling panel focus and tray-driven bring-to-front behavior
- positioning a popup near the Windows tray area
- using `Task` and subscriptions for background refresh work
- building a compact custom-styled popup with `iced`
- storing local JSON config/cache state with `serde`
- keeping provider-specific fetch logic separate from app UI state

## How it works

Usage Radar stays intentionally simple.

- `src/main.rs` boots an `iced::daemon`
- `src/app/` owns the main state, refresh loop, UI rendering, and user interactions
- `src/tray/` owns the tray icon and tray menu wiring
- `src/panel/` owns popup window sizing and positioning
- `src/providers/` owns provider-specific fetch/parsing logic
- `src/storage/` owns local JSON config/cache persistence
- `src/util/` owns small path helpers

Provider adapters normalize their source data into a shared `ProviderSnapshot` shape. The app then owns display logic, refresh orchestration, tray state, and cache state.

## Local data and auth

Usage Radar keeps its local state on Windows here:

- Config: `%APPDATA%\UsageRadar\config.json`
- Cache: `%LOCALAPPDATA%\UsageRadar\snapshots.json`

Auth details:

- Codex auth is read from `%USERPROFILE%\.codex\auth.json` or `CODEX_HOME\auth.json`
- GitHub Copilot uses GitHub device flow
- The saved Copilot token is stored in Windows credential storage, not in the app config/cache JSON

## Run locally

### Requirements

- Windows 10 or Windows 11
- Rust stable toolchain
- Codex installed and signed in if you want Codex data
- A GitHub account with Copilot access if you want Copilot data

### Run

```bash
cargo run
```

### Check

```bash
cargo fmt && cargo check
```

### Build

```bash
cargo build --release
```

The release binary will be:

```text
target/release/usage-radar.exe
```

## Download release builds

This repo includes a GitHub Actions release workflow.

- Manual workflow runs build a Windows release zip and upload it as a workflow artifact
- Tagged releases like `v0.1.0` build the Windows zip and attach it to GitHub Releases automatically

Release artifacts are packaged as:

```text
usage-radar-<version>-windows-x64.zip
```

Each archive contains:

- `usage-radar.exe`
- `README.md`
- `LICENSE`

## Repo map

```text
usage-radar/
├── assets/
├── docs/
│   └── plans/
├── src/
│   ├── app/
│   ├── panel/
│   ├── providers/
│   ├── storage/
│   ├── tray/
│   ├── util/
│   └── main.rs
├── MVP.md
├── SPEC.md
└── Cargo.toml
```

If you are new to the codebase, start with:

- `MVP.md`
- `SPEC.md`
- `src/main.rs`
- `src/app/mod.rs`

## Limitations

- Windows-first only right now
- Provider support is only as stable as the underlying provider surfaces
- Codex and Copilot may change their auth or usage endpoints in the future
- Claude Code and Gemini CLI are not wired yet
- The UI is intentionally compact and still evolving

## Roadmap

Near-term:

- keep polishing the compact tray popup UX
- add trustworthy provider integrations only when real sources exist
- improve release polish and onboarding for first-time users
- keep the codebase understandable for contributors and `iced` learners

## Contributing

Issues and PRs are welcome.

A good way to contribute is to keep the project aligned with its core shape:

- tray-first
- compact
- Windows-first
- honest about freshness and confidence
- calm code over clever abstractions

## License

MIT.
