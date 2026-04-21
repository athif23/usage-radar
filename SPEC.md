# Usage Radar Technical Spec

## 1. Purpose

This document translates the MVP plan into implementation-oriented technical structure.

It defines:
- tray and panel mechanics
- module ownership
- provider adapter shape
- state ownership
- persistence rules
- refresh behavior
- confidence/staleness behavior
- UI structure
- guardrails

This spec should be detailed enough to guide scaffolding and early implementation without turning into a frozen architecture prison.

---

## 2. System Overview

Usage Radar is a single desktop app with provider adapters.

```text
Rust desktop app
  ├─ tray integration
  ├─ compact panel window
  ├─ app state + refresh loop
  ├─ provider adapters
  └─ local cache/config
```

### Why this shape
- The product is small and local-first.
- A separate backend or sidecar is unnecessary for MVP.
- Each provider can be integrated through its own adapter.
- The tray and compact panel remain the center of the experience.

### Current target providers
- Codex
- GitHub Copilot
- OpenCode Go

### Future providers
- Claude Code

---

## 3. Runtime Boundaries

## 3.1 Desktop app responsibilities (`src/`)

The Rust desktop app owns:
- tray lifecycle
- panel window lifecycle
- summary rendering
- provider detail rendering
- refresh scheduling
- cached snapshot loading/saving
- stale/confidence display state
- provider enablement visibility rules
- user interactions like refresh/open/close

The desktop app does **not** own:
- provider billing semantics
- provider account management
- full dashboard analytics
- credentials beyond what MVP absolutely needs

## 3.2 Provider adapter responsibilities (`src/providers/`)

Each provider adapter owns:
- source discovery
- provider-specific fetch logic
- provider-specific parsing
- provider-specific normalization into app snapshots
- confidence classification (`Exact`, `Estimated`, `Partial`)

A provider adapter should be thin.
It should not become a mini-framework.

---

## 4. Ownership Rules

## 4.1 Provider-owned canonical facts

These come from the provider source itself or a strongly bounded local source:
- current usage numbers
- reset timing
- subscription window facts
- provider-supported metadata

## 4.2 App-owned data

The app owns:
- tray state
- panel open/closed state
- selected provider tab
- local cached snapshots
- refresh timestamps
- UI-only urgency ordering
- config/preferences used by the app
- logs

## 4.3 Conflict rule

If cached data and newly fetched provider data disagree, the fresh provider data wins.

---

## 5. Repo Structure

```text
usage-radar/
├── docs/
│   └── plans/
├── src/
│   ├── main.rs
│   ├── app/
│   ├── tray/
│   ├── panel/
│   ├── providers/
│   ├── storage/
│   ├── widgets/
│   ├── theme/
│   └── util/
├── assets/
├── scripts/
├── MVP.md
├── SPEC.md
├── README.md
└── .gitignore
```

### 5.1 Suggested module structure

```text
src/
├── main.rs
├── app/
│   ├── mod.rs
│   ├── state.rs
│   ├── message.rs
│   ├── update.rs
│   ├── commands.rs
│   └── startup.rs
├── tray/
│   ├── mod.rs
│   ├── icon.rs
│   ├── menu.rs
│   └── events.rs
├── panel/
│   ├── mod.rs
│   ├── window.rs
│   ├── layout.rs
│   ├── summary.rs
│   └── detail.rs
├── providers/
│   ├── mod.rs
│   ├── kinds.rs
│   ├── shared.rs
│   ├── codex.rs
│   └── copilot.rs
├── storage/
│   ├── mod.rs
│   ├── config.rs
│   ├── cache.rs
│   └── logs.rs
├── widgets/
│   ├── mod.rs
│   ├── progress_bar.rs
│   ├── provider_row.rs
│   ├── provider_tab.rs
│   └── status_badge.rs
├── theme/
│   ├── mod.rs
│   └── palette.rs
└── util/
    ├── mod.rs
    ├── time.rs
    ├── format.rs
    └── paths.rs
```

### 5.2 Structure rule

Keep the names concrete and product-shaped.
Avoid generic “enterprise” layering names.

Good names here are things like:
- `tray`
- `panel`
- `providers`
- `storage`
- `widgets`

Not abstract labels that hide what the code really does.

---

## 6. App Architecture

## 6.1 Iced application model

The desktop app should follow a straightforward iced structure:
- one top-level `App` state
- one top-level `Message` enum
- explicit `update` function
- small view builders
- async work through tasks/subscriptions

This keeps state movement obvious and the code easy to skim.

## 6.2 Root app state

Illustrative shape:

```rust
use std::collections::HashMap;
use std::time::Instant;

pub struct App {
    pub tray: tray::State,
    pub panel: panel::State,
    pub providers: HashMap<ProviderKind, ProviderSnapshot>,
    pub config: storage::config::AppConfig,
    pub cache: storage::cache::CacheState,
    pub refresh: RefreshState,
}

pub struct RefreshState {
    pub in_flight: bool,
    pub queued: bool,
    pub last_started_at: Option<Instant>,
    pub last_finished_at: Option<Instant>,
}
```

## 6.3 Root app message enum

Illustrative shape:

```rust
pub enum Message {
    AppStarted,
    TrayLeftClicked,
    TrayRightClicked,
    TrayMenuOpen,
    TrayMenuRefresh,
    TrayMenuQuit,

    TogglePanel,
    ClosePanel,
    FocusSummary,
    SelectProvider(ProviderKind),

    RefreshRequested(RefreshReason),
    RefreshFinished(Result<Vec<ProviderSnapshot>, AppError>),

    ProviderTabSelected(ProviderKind),
    CacheLoaded(Result<Vec<ProviderSnapshot>, AppError>),

    WindowFocused(bool),
    EscapePressed,
}
```

## 6.4 Update loop rule

`update` should stay boring:
- mutate app state synchronously
- return a task for async work
- map provider fetch results back into app messages
- avoid putting provider business logic into the view layer

---

## 7. Tray and Panel Mechanics

## 7.1 Shell strategy

For MVP, treat the compact panel as a small utility window, not a native context menu.

Recommended shape:
- tray icon handled by a dedicated tray crate such as `tray-icon` citeturn627106search1turn627106search13
- panel rendered by iced
- panel window is reused, not recreated each open
- panel is undecorated, compact, and dismissible
- `Esc` closes the panel
- outside-click dismissal is desirable if feasible, but not required to block MVP

## 7.2 Window behavior

The panel window should:
- be hidden until needed
- open near the tray area when possible
- avoid taskbar clutter if feasible
- feel instant to open because it is already created and only shown/positioned

### Future note
iced supports window level concepts such as `AlwaysOnTop`, which is relevant for later widget mode, but that mode is out of scope for MVP citeturn627106search3turn627106search11

## 7.3 Tray menu

Minimum tray menu actions:
- Open
- Refresh
- Quit

Do not expand this into a settings hub in MVP.

---

## 8. UI Architecture

## 8.1 Panel hierarchy

The UI should be composed in layers:
1. panel shell
2. header/meta row
3. summary list
4. provider tabs
5. selected provider detail
6. footer/meta state if needed

## 8.2 Header

Must surface:
- app title or compact label
- last updated state
- refresh control

## 8.3 Summary

The summary is the first-class surface.

Each row should contain:
- provider name
- primary text in **percent left**
- progress bar in **percent used**
- reset timing
- optional subline
- confidence/stale indicators if relevant

Recommended row shape:
- main: `Codex · 18% left · resets in 42m`
- sub: `Weekly safe`

## 8.4 Provider detail

The selected provider detail should show the real provider shape.

### Codex detail
- 5h usage bar
- 5h reset time
- weekly usage bar
- weekly reset time
- support/confidence state

### Copilot detail
- premium usage bar
- cycle/reset timing
- remaining count if available
- percentage fallback if exact count is unavailable
- support/confidence state

## 8.5 Visual tone
- dark by default
- compact
- restrained contrast
- no dashboard feel
- no oversized cards or empty spacing

---

## 9. Provider Model

## 9.1 Definitions

- **Provider**: Codex, Copilot, or future source integrated by the app
- **Snapshot**: normalized provider state used by the UI
- **Limit bar**: one concrete usage bar for a provider
- **Confidence**: exactness quality of the displayed data

## 9.2 Provider kind

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    Codex,
    Copilot,
    ClaudeCode,
    OpenCodeGo,
}
```

## 9.3 Confidence model

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    Exact,
    Estimated,
    Partial,
}
```

### Confidence rules
- **Exact**: derived from trusted provider values or strongly bounded local sources
- **Estimated**: inferred from incomplete data or derived math
- **Partial**: some useful values exist, but important fields are missing or weak

### UI rules
- `Exact`: no special badge unless stale
- `Estimated`: show a small `Estimated` marker
- `Partial`: show a small `Partial data` marker

## 9.4 Limit bar shape

```rust
pub struct LimitBar {
    pub label: String,
    pub percent_used: f32,
    pub percent_left: f32,
    pub reset_at: Option<std::time::SystemTime>,
    pub subtitle: Option<String>,
}
```

## 9.5 Provider snapshot shape

```rust
pub struct ProviderSnapshot {
    pub kind: ProviderKind,
    pub visible: bool,
    pub confidence: Confidence,
    pub fetched_at: std::time::SystemTime,
    pub stale: bool,
    pub unavailable: bool,
    pub summary_bar: Option<LimitBar>,
    pub detail_bars: Vec<LimitBar>,
    pub notes: Vec<String>,
}
```

---

## 10. Urgency and Summary Rules

## 10.1 Thresholds

- normal: `> 15% left`
- warning: `<= 15% left`
- critical: `<= 5% left`

## 10.2 Summary row rules

- text uses **percent left**
- bar uses **percent used**
- row uses the provider’s most urgent usable bar
- optional subline can clarify secondary status

## 10.3 Codex summary rule

If both 5h and weekly windows exist:
- detail view always shows both
- summary chooses the more urgent window

Illustrative helper:

```rust
fn pick_codex_summary_bar(snapshot: &ProviderSnapshot) -> Option<&LimitBar> {
    snapshot
        .detail_bars
        .iter()
        .min_by(|a, b| a.percent_left.total_cmp(&b.percent_left))
}
```

## 10.4 Summary sorting rule

Sort visible providers by:
1. urgency tier
2. lowest `percent_left`
3. nearest `reset_at`
4. stable provider name fallback

Rows without known reset time should sort after rows with known reset time when urgency is otherwise similar.

---

## 11. Refresh Model

## 11.1 Trigger rules

Refresh should happen:
- on app startup after cache load
- on panel open
- every 5 minutes
- on explicit manual refresh

## 11.2 Concurrency rules

Only one refresh cycle may be active at a time.

If a new trigger happens during an active refresh:
- mark `queued = true`
- when the current refresh finishes, run one follow-up refresh if still needed
- do not allow refresh storms from timer + open + manual spam

Illustrative helper:

```rust
fn request_refresh(state: &mut RefreshState) -> bool {
    if state.in_flight {
        state.queued = true;
        false
    } else {
        state.in_flight = true;
        true
    }
}
```

## 11.3 Partial failure rules

If one provider fails but another succeeds:
- keep the successful provider fresh
- keep failed provider at last-known value if still within grace period
- mark failed provider stale
- move to unavailable only when too old

---

## 12. Staleness and Unavailable Rules

## 12.1 Staleness model

Recommended MVP states:
- fresh
- stale
- unavailable

### Suggested rule
- `fresh`: within expected refresh window
- `stale`: refresh failed or too much time passed, but last-known value still useful
- `unavailable`: no trustworthy value remains

## 12.2 UI behavior

- stale data remains visible with a clear label and last updated time
- unavailable providers remain visible only if support is expected and the empty state is informative
- unsupported future providers should not clutter MVP UI

## 12.3 Tray icon precedence

For MVP:
- tray warning dot only reflects warning/critical usage state
- stale/unavailable does not change the tray icon yet
- stale/unavailable is communicated inside the panel

---

## 13. Provider Access Strategy

## 13.1 General rule

Use the least invasive source that produces useful data.

Preferred order:
1. local trusted tool state or local auth/session discovery
2. provider-supported APIs or endpoints when practical
3. clearly labeled estimation

## 13.2 Codex

MVP goal:
- strong support
- daily-usable trust level

Recommended approach:
- inspect how working tools such as CodexBar and PeekaUsage obtain Codex data
- prefer local or official-enough read-only paths first
- classify output as `Exact` only when the source is strong enough

## 13.3 Copilot

MVP goal:
- useful if possible, but not at the cost of fake certainty

Important rule:
- Copilot support is **provisional** in MVP
- exact remaining counts are nice to have, not mandatory
- percentage-only fallback is acceptable
- if support weakens, show honest partial/unavailable state instead of pretending parity with Codex

## 13.4 Security rules

- do not persist raw tokens in cache files
- do not log secrets
- keep discovery read-only when possible
- if manual auth is ever added, store secrets in OS credential storage instead of plain files citeturn627106search2turn627106search18

---

## 14. Persistence Model

## 14.1 Config and cache paths

Recommended split on Windows:
- config: `%APPDATA%/UsageRadar/config.json`
- cache: `%LOCALAPPDATA%/UsageRadar/snapshots.json`
- logs: `%LOCALAPPDATA%/UsageRadar/logs/...`

## 14.2 Config contents

MVP config should stay small.

Illustrative shape:

```rust
pub struct AppConfig {
    pub selected_provider: Option<ProviderKind>,
    pub refresh_minutes: u64,
    pub start_in_tray: bool,
}
```

## 14.3 Cache contents

Cache should store only what helps startup feel immediate.

Illustrative shape:

```rust
pub struct CachedSnapshots {
    pub version: u32,
    pub providers: Vec<ProviderSnapshot>,
}
```

Should not store:
- raw auth tokens
- unnecessary provider payload dumps
- brittle derived data that can be recomputed safely

---

## 15. Example View Composition

Illustrative summary row builder:

```rust
fn provider_row(snapshot: &ProviderSnapshot) -> Element<Message> {
    let Some(bar) = &snapshot.summary_bar else {
        return text(format!("{} · unavailable", snapshot.kind.label())).into();
    };

    let main = format!(
        "{} · {:.0}% left{}",
        snapshot.kind.label(),
        bar.percent_left,
        format_reset_suffix(bar.reset_at),
    );

    column![
        text(main),
        widgets::progress_bar::usage(bar.percent_used),
        maybe_subline(bar.subtitle.as_deref()),
        maybe_confidence_badge(snapshot.confidence, snapshot.stale),
    ]
    .spacing(6)
    .into()
}
```

Illustrative provider tabs section:

```rust
fn provider_tabs(app: &App) -> Element<Message> {
    let tabs = app
        .providers
        .values()
        .filter(|snapshot| snapshot.visible)
        .map(|snapshot| {
            widgets::provider_tab::tab(
                snapshot.kind.label(),
                app.panel.selected_provider == Some(snapshot.kind),
                Message::SelectProvider(snapshot.kind),
            )
        });

    row(tabs).spacing(8).into()
}
```

---

## 16. Build Order

## 16.1 Step 1 — Codex vertical slice

Ship one trustworthy vertical slice first:
- tray icon
- compact panel
- Codex adapter
- cached snapshot
- stale labeling
- on-open refresh

## 16.2 Step 2 — Summary polish

Then add:
- urgency sorting
- better row visuals
- warning dot
- manual refresh affordance

## 16.3 Step 3 — Copilot integration

Then add:
- Copilot adapter
- confidence/partial handling
- percentage fallback
- honest degraded states

## 16.4 Step 4 — small refinements

Then add:
- better tray menu
- more polished panel sizing/positioning
- better empty/unavailable states

---

## 17. Guardrails

- Keep the app tray-first.
- Keep the panel compact.
- Keep names concrete and readable.
- Keep provider logic inside provider adapters.
- Keep summary logic honest.
- Keep stale/partial states explicit.
- Avoid generic architecture theater.
- Avoid building empty folders/modules before a real need exists.
- Build Codex end-to-end before chasing broad provider coverage.

---

## 18. Open Technical Questions

1. What exact Codex source will be the strongest starting point for MVP?
2. What exact Copilot source is stable enough to be useful in MVP?
3. What is the cleanest tray-positioning strategy on Windows for the panel?
4. Is outside-click dismissal easy enough in the chosen tray/window approach, or should it wait?
5. How much provider-source-specific retry logic is worth adding before MVP drifts too far?

---

## 19. Definition of a Good Spec Outcome

This spec is doing its job if it makes the following clear:
- what the app is really building
- what the tray and panel do
- how providers plug in
- how fresh/stale/unavailable states behave
- what gets built first
- what must not drift during implementation
