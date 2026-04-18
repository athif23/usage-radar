---
title: "feat: Usage Radar Slice Plan"
type: feat
status: active
date: 2026-04-18
origin:
  - MVP.md
  - SPEC.md
---

# feat: Usage Radar Slice Plan

## Purpose

This is the **working implementation plan** for Usage Radar.

Unlike the MVP doc, which defines the full product target, this document defines the **build order** and the **smallest trustworthy slices** we should implement one by one.

The goal is to keep the project honest:
- prove the tray loop early
- prove provider trust before breadth
- avoid building polish around unproven data sources
- keep the app native-feeling, compact, and calm

## Planning Rules

1. **Slices must prove something real**
   - each slice should reduce uncertainty, not just add structure
2. **Do not skip forward for excitement**
   - Copilot and polish are tempting, but not before the Codex loop is trustworthy
3. **Do not broaden a slice once it starts**
   - defer tempting extras to later slices
4. **Trust beats prettiness**
   - honest stale/partial handling matters more than polished visuals
5. **Prefer product proof over architecture cleverness**
   - a working small slice beats a broad elegant skeleton

---

## Global Architecture Guardrails

These apply to every slice:

- The app is tray-first
- The panel stays compact and utility-like
- The first answer should be visible on open without extra clicks
- Provider adapters own fetch/parse/normalize logic
- App state owns cache, refresh, tray, panel, and display behavior
- Stale, estimated, and partial states must be explicit
- Avoid fake-unified provider semantics in detail views
- Build Codex end-to-end before chasing broad provider coverage

---

## Slice Overview

| Slice | Goal |
|------|------|
| Slice 0 | Repo + app shell + local persistence |
| Slice 1 | Tray shell + compact panel boot |
| Slice 2 | Codex vertical slice |
| Slice 3 | Summary polish + urgency behavior |
| Slice 4 | Copilot provisional integration |
| Slice 5 | Tray/panel refinement |

---

# Slice 0 — Repo + app shell + local persistence

## Goal

Prove the repo shape and the basic Iced app lifecycle.

By the end of this slice, we should be able to boot the app, load local config/cache, and keep the project structure stable enough for the tray and provider work that follows.

## Why this slice exists

Before tray mechanics or provider adapters, we need confidence that:
- the app boots cleanly
- local persistence paths are clear
- state ownership is obvious
- the repo shape is calm and durable

## In scope

### Repo scaffold
- create/confirm repo shape:
  - `src/`
  - `docs/plans/`
  - `assets/`
  - `MVP.md`
  - `SPEC.md`
  - `AGENTS.md`

### App scaffold
- minimal Iced app boots
- dark base theme placeholder exists
- top-level `App` state + `Message` enum skeleton exists
- startup path can load cached snapshots and config

### Persistence scaffold
- config path resolved cleanly
- cache path resolved cleanly
- config and cache structs exist
- empty/missing file behavior is safe

## Explicitly out of scope
- tray icon integration
- provider networking
- Codex parsing
- Copilot support
- advanced panel visuals

## Deliverables
- minimal app boot
- config/cache path helpers
- config/cache file load/save scaffold
- stable repo shape

## Acceptance criteria
- app boots reliably
- missing config/cache does not cause startup failure
- app-owned state is easy to locate
- repo structure is stable enough to continue building on

## Failure signals
- startup state already feels overcomplicated
- persistence ownership is unclear
- repo shape is becoming broader than the product needs

---

# Slice 1 — Tray shell + compact panel boot

## Goal

Prove the primary product interaction:
- app runs in tray
- left-click opens a compact panel
- panel closes predictably
- cached state can appear immediately

## Why this slice exists

If tray open/close behavior feels flaky or slow, the product fails before provider data even matters.

## In scope

### Tray shell
- tray icon boots reliably on Windows
- left-click toggles panel
- right-click opens a tiny menu with:
  - Open
  - Refresh
  - Quit

### Panel shell
- compact utility window exists
- panel is reused, not recreated on every open
- dark compact placeholder layout exists
- `Esc` dismiss works

### Early state behavior
- cached placeholder or empty snapshot state can render immediately
- manual refresh action exists at the shell level, even if provider wiring is still stubbed

## Explicitly out of scope
- real Codex fetch logic
- Copilot support
- urgency sorting
- outside-click dismissal if it complicates the slice
- polished tray warning state

## Deliverables
- tray icon
- reusable compact panel window
- basic menu
- panel open/close loop

## Acceptance criteria
- app can live in tray reliably
- left-click open feels near-instant
- right-click menu works reliably
- `Esc` dismiss works predictably
- panel does not feel like a recreated web page

## Failure signals
- tray behavior is flaky on Windows
- panel positioning or focus becomes brittle
- open/close loop feels heavier than expected

---

# Slice 2 — Codex vertical slice

## Goal

Prove the first trustworthy provider loop:
- open panel
- see last-known Codex state immediately
- refresh Codex on open or manually
- show 5h and weekly detail bars honestly

## Why this slice exists

The project becomes real only when one provider is trustworthy enough for daily use.

## In scope

### Codex adapter
- identify the strongest realistic MVP source
- fetch Codex usage data
- normalize into app snapshot shape
- classify confidence honestly

### Codex UI
- one summary row
- one detail view with:
  - 5h bar
  - 5h reset time
  - weekly bar
  - weekly reset time
  - confidence/support state

### Refresh behavior
- startup refresh after cache load
- on-open refresh
- manual refresh
- one refresh cycle at a time

### Trust behavior
- show cached snapshot immediately
- keep failed refresh at last-known value during grace period
- mark stale clearly
- mark unavailable when too old

## Explicitly out of scope
- Copilot support
- multi-provider urgency sorting
- tray warning dot
- advanced empty states
- history/dashboard features

## Deliverables
- Codex adapter
- Codex snapshot persistence
- summary row
- detail view
- refresh/staleness handling

## Acceptance criteria
- Codex data is useful enough to trust daily
- cached state appears immediately
- refresh does not block panel open
- stale and unavailable states are visually unambiguous
- the app already feels lighter than checking a browser/dashboard

## Failure signals
- Codex source is too weak to trust
- stale data handling is confusing
- refresh behavior feels fragile or race-prone

## Notes / risks
- do not broaden to other providers to hide uncertainty here
- the Codex source decision may force plan revisions

---

# Slice 3 — Summary polish + urgency behavior

## Goal

Make the first-open summary feel decision-ready.

By the end of this slice, the summary should make it obvious what is most constrained and when it resets.

## Why this slice exists

Even with one provider working, the product promise is about fast-glance decisions. The summary needs to feel strong before we broaden provider coverage.

## In scope

### Summary rules
- primary text uses percent left
- bars use percent used
- urgency thresholds:
  - warning at 15% left
  - critical at 5% left
- Codex summary uses the most constrained usable bar

### Visual polish
- better row hierarchy
- clearer refresh/last-updated state
- calm footer/meta handling
- restrained compact styling

### Tray warning
- neutral tray icon by default
- warning dot when current provider state reaches warning/critical

## Explicitly out of scope
- Copilot support
- notification system
- charts/history
- settings expansion

## Deliverables
- polished summary row
- urgency helper logic
- warning-state tray icon behavior

## Acceptance criteria
- the important answer is visible in seconds
- warning/critical behavior feels consistent
- tray signal is helpful without becoming noisy

## Failure signals
- summary becomes pretty but decision-poor
- styling starts drifting into dashboard energy

---

# Slice 4 — Copilot provisional integration

## Goal

Add a second provider without pretending parity with Codex.

## Why this slice exists

The MVP wants Codex and Copilot, but the product explicitly says Copilot must stay honest if the source is weaker.

## In scope

### Copilot adapter
- identify the strongest realistic MVP source
- fetch whatever useful current-cycle data is available
- normalize into app snapshot shape
- classify `Exact`, `Estimated`, or `Partial`

### Copilot UI
- summary row
- detail view with:
  - premium usage bar
  - current cycle/reset timing
  - remaining count if available
  - percentage fallback if exact count is unavailable
  - support/confidence state

### Cross-provider behavior
- visible provider ordering by urgency
- partial failure handling across providers
- honest support labels

## Explicitly out of scope
- forcing Copilot into fake detail parity with Codex
- notifications
- account/billing management
- more providers

## Deliverables
- Copilot adapter
- Copilot summary/detail view
- two-provider summary ordering

## Acceptance criteria
- Copilot support is useful even if partial
- the UI does not imply more confidence than the source deserves
- Codex quality does not regress while adding Copilot

## Failure signals
- Copilot display feels misleading
- cross-provider summary logic becomes harder to trust
- broadening to Copilot exposes weak snapshot assumptions

## Notes / risks
- if the source is too weak, degrade honestly rather than forcing support

---

# Slice 5 — Tray/panel refinement

## Goal

Make the product feel native and habitual.

## Why this slice exists

Once the core provider loop is trustworthy, the next most important thing is habitability: speed, dismissal, sizing, and calm interaction.

## In scope

### Tray/panel refinement
- better panel sizing and positioning
- better focus behavior
- outside-click dismissal if feasible without destabilizing the shell
- stronger empty/unavailable states

### Small UX improvements
- selected provider persistence
- slightly better footer/meta copy
- more polished compact layout spacing

## Explicitly out of scope
- widget mode
- notifications
- account management
- plugin system
- Linux/macOS parity

## Deliverables
- more polished tray/panel behavior
- better empty and unavailable handling
- improved session-free startup feel

## Acceptance criteria
- the app feels fast enough to become habitual
- dismissal/opening feels low-friction
- the panel still feels compact and calm
- refinements do not add bloat

## Failure signals
- refinement work adds too much chrome
- window behavior becomes less predictable
- the panel starts drifting toward a general dashboard

---

## Cross-Slice Exit Rules

A slice is done only when:
1. the slice goal is proven in normal use
2. the acceptance criteria are met
3. the exact area the slice was meant to prove does not feel fragile
4. we did not smuggle too many later-slice features into it

If a slice proves the current architecture is wrong, revise the plan instead of blindly continuing.

---

## Working Priorities

When tradeoffs appear during implementation, prioritize in this order:

1. trustworthiness of provider data
2. speed of tray and panel interaction
3. correctness of stale/partial/unavailable behavior
4. simplicity of implementation
5. visual polish
6. breadth of provider support

---

## Definition of a Good Plan Outcome

This plan is successful if it helps us build Usage Radar in a way that feels:
- tray-first
- fast-glance first
- honest
- native-feeling
- slice-proven instead of feature-bloated

And especially if it stops us from trying to build the whole MVP at once.
