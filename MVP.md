# Usage Radar MVP

## 1. Product Summary

Usage Radar is a tray-first Windows desktop app for checking AI usage limits quickly.

It exists to remove the guesswork around daily use of tools like Codex and GitHub Copilot.
Instead of opening web dashboards, terminals, or account pages, the user should be able to:
- glance at current risk,
- see what is closest to exhaustion,
- see when each limit resets,
- and decide which tool is safe to use right now.

Usage Radar is **not** an analytics dashboard, account manager, or browser wrapper.
It is a small native utility whose main job is fast, trustworthy visibility.

---

## 2. MVP Goal

Deliver a Windows-first tray utility in Rust with iced that lets a user:
1. keep the app running in the tray,
2. left-click to open a compact native panel,
3. immediately see which provider is closest to exhaustion,
4. inspect Codex and Copilot details without opening other apps,
5. trust stale, partial, and estimated states because they are labeled honestly,
6. use it multiple times a day because it opens fast and feels light.

If the app feels like “my AI limits, without the guessing or dashboard friction,” the MVP is working.

---

## 3. Product Principles

1. **Tray-first**
   - The tray is the primary home of the app.
   - The main interaction starts from the tray icon, not a large main window.

2. **Fast-glance first**
   - The first open should answer the important question immediately.
   - The user should not need extra clicks to see what matters.

3. **Trust over prettiness**
   - A simple honest number beats a polished but misleading display.
   - Stale, partial, or estimated values must be shown as such.

4. **Native utility feel**
   - It should feel like a small Windows utility, not a web app in a box.
   - Open fast, dismiss fast, low chrome.

5. **Provider-aware, not fake-unified**
   - The summary should feel unified.
   - The details should still respect that each provider exposes different limit shapes.

6. **Small before broad**
   - V1 should solve Codex and Copilot well before chasing every provider.
   - Future support matters, but not at the cost of a weak first release.

7. **Desktop-specific value only**
   - The app should add convenience, speed, and habitability.
   - It should not recreate full account pages or billing dashboards.

---

## 4. Problem Statement

AI usage limits now live in too many places:
- web dashboards,
- CLI tools,
- account pages,
- internal usage pages,
- local tool state,
- and sometimes nowhere obvious at all.

That creates daily friction.

### Main pain points
- the user has to guess whether a provider is safe to use
- there is no single fast place to check current exhaustion risk
- reset times are not always easy to find
- switching between tools wastes time
- browser/dashboard workflows feel too heavy for a quick check
- some tools expose only partial information, making confidence unclear

### Desired improvement
The app should let the user:
- know current limit risk quickly,
- spot the most constrained provider immediately,
- understand reset timing at a glance,
- avoid hitting caps unexpectedly,
- and make better day-to-day usage decisions.

---

## 5. Target User

### Primary user
A developer who:
- actively uses Codex and GitHub Copilot,
- often works near subscription or quota limits,
- wants a tray utility instead of a dashboard,
- cares about speed and honesty,
- is on Windows first.

### Secondary user
A multi-provider AI power user who wants a calm operator panel for checking current usage without opening multiple tools.

---

## 6. MVP Scope

### Must-have product capabilities
- tray icon
- left-click opens compact panel
- right-click opens tiny menu
- unified summary sorted by urgency
- provider rows with:
  - percent left in text
  - progress bar showing percent used
  - reset time
  - optional small subline when useful
- Codex detail section
- GitHub Copilot detail section
- refresh on interval
- refresh on panel open
- manual refresh
- stale data labeling
- estimated / partial support labeling
- cached last-known snapshot for quick startup
- keyboard dismiss (`Esc`)

### First-class surfaces required in MVP
- tray icon
- compact summary panel
- provider detail tabs inside the same panel
- tiny tray context menu

### Explicit non-goals for MVP
- notifications
- multiple accounts
- heavy settings UI
- charts/history dashboard
- account/billing management
- browser embedding
- always-on-top transparent widget mode
- plugin marketplace
- Linux/macOS parity
- deep customization

---

## 7. First Playable Slice

The MVP is broader than the first implementation slice.

### First playable slice goal
One provider, one tray icon, one compact panel, one trustworthy refresh loop.

### First playable slice includes
- app launches to tray
- tray icon can open and close a compact panel
- Codex only
- one summary row driven by real provider data
- one detail panel for Codex showing both 5h and weekly windows
- on-open refresh
- manual refresh
- cached last-known snapshot
- stale labeling

### Explicitly deferred from first playable slice
- Copilot support
- urgency sorting across multiple providers
- partial/estimated cross-provider UI variations
- richer tray menu
- persistent settings UI
- future widget mode

### Why this matters
If the app tries to solve all providers before one end-to-end path is trustworthy, it becomes broad before it becomes dependable.

---

## 8. Core User Flows

### Flow A — Quick tray check
1. App is already running in tray
2. User left-clicks tray icon
3. Compact panel opens instantly
4. User sees summary sorted by urgency
5. User closes it with click-away or `Esc`

### Flow B — Decide which provider is safe
1. User opens the panel
2. Summary shows the most constrained provider first
3. User compares Codex and Copilot quickly
4. User decides which tool to spend next

### Flow C — Inspect provider detail
1. User opens the panel
2. User checks unified summary first
3. User clicks a provider tab
4. User sees provider-specific bars and reset timing
5. User returns to work without opening external dashboards

### Flow D — Recover from stale state
1. User opens the panel
2. The last known snapshot appears immediately
3. A refresh begins automatically
4. If refresh fails, the data remains visible but marked stale
5. If too old, provider becomes unavailable instead of pretending confidence

---

## 9. UX Shape

### Core layout
The compact panel should contain:
1. panel header
2. unified summary list
3. provider tabs
4. selected provider detail view
5. small footer/meta area for refresh state

### Panel header
Should provide:
- app name or minimal title
- last updated / refresh state
- manual refresh action

### Unified summary
Each provider row should show:
- provider name
- primary limit text in **percent left**
- progress bar in **percent used**
- reset timing
- optional subline when useful

Recommended row shape:
- `Codex · 18% left · resets in 42m`
- small muted subline: `Weekly safe`

### Provider details
Should show the provider’s real limit shape, not a forced fake dashboard schema.

For Codex:
- 5h bar
- weekly bar
- reset times
- support state

For Copilot:
- premium usage bar
- current cycle timing
- remaining or percentage fallback
- support state

### Visual hierarchy
Priority of visual weight:
1. summary rows
2. selected provider detail
3. panel header
4. misc controls/chrome

### Design tone
- dark by default
- compact
- restrained contrast
- no dashboard energy
- no oversized cards
- no heavy navigation
- should feel like a small operator utility

### Scroll behavior
Scroll is acceptable, but the panel should not feel scroll-heavy.
The first open should still expose the important state without requiring scrolling.

---

## 10. Behavioral Rules

### Summary rules
- rows are sorted by urgency
- summary text uses **percent left**
- bars use **percent used**
- warning threshold is **15% left**
- critical threshold is **5% left**
- Codex summary should use the nearest constrained window
- Copilot summary may use percentage fallback if exact count is unavailable

### Refresh rules
- refresh every 5 minutes
- refresh on panel open
- manual refresh is available
- only one refresh cycle may be in flight at a time
- last-known snapshot should be shown immediately on open

### Staleness rules
- stale data should remain visible for a grace period
- stale state must be labeled clearly
- if data becomes too old, mark provider unavailable
- never display stale data as fresh

### Confidence rules
- **Exact** = trusted provider value or strongly bounded local source
- **Estimated** = inferred value or incomplete source math
- **Partial** = some metrics available, but key fields missing or weak

### Tray rules
- neutral tray icon by default
- show warning dot when any provider is in warning or critical
- left click opens/closes the compact panel
- right click opens a tiny menu with at least:
  - Open
  - Refresh
  - Quit

### Ownership rules
- provider sources own canonical usage facts
- the app owns local cache, tray state, panel state, and display logic
- if a refresh result disagrees with cached data, the new provider data wins

---

## 11. Performance Bar

The MVP should feel fast enough that the user actually builds a habit around it.

### Performance expectations
- tray click should feel near-instant
- cached summary should appear immediately
- panel should not feel like it is booting a web page
- refresh should not block panel open
- keyboard dismiss should feel instant
- summary scanning should take only a few seconds
- normal interaction should feel lighter than opening a browser tab

---

## 12. Suggested Stack

### Core app
- **Rust** for the desktop app
- **iced** for the native GUI and panel rendering, which supports cross-platform GUI development, custom widgets, async actions, and Windows support citeturn627106search0turn627106search20

### Tray and shell integration
- a dedicated tray crate such as **tray-icon**, which supports Windows tray icons and is meant specifically for desktop tray applications citeturn627106search1turn627106search13
- a small undecorated utility window for the panel, reused instead of recreated each open

### Persistence and caching
- JSON files for config and cached snapshots in MVP
- local cache path for snapshots/logs
- roaming config path for lightweight config

### Networking / provider access
- provider-specific adapters in Rust
- start with read-only access patterns
- avoid storing raw secrets unless manual auth is added later

### Secret handling
- if manual credentials are added later, use OS credential storage via a crate such as **keyring**, which targets platform-secure stores including Windows credential storage through the keyring ecosystem citeturn627106search2turn627106search6turn627106search18

---

## 13. Success Criteria

The MVP is successful when all of these feel true:

1. The app opens from the tray fast enough to feel habitual.
2. The user checks it multiple times a day instead of guessing.
3. The user stops unexpectedly hitting usage caps as often.
4. The summary makes it obvious what is closest to exhaustion.
5. Codex data is trustworthy enough to rely on daily.
6. Copilot support is useful even if some metrics are partial.
7. Stale and partial states feel honest instead of confusing.
8. The app feels native and lightweight on Windows.

---

## 14. Failure Criteria

The MVP should be considered weak or wrong if:
- the panel opens slowly
- the numbers feel unreliable
- stale data looks fresh
- the app feels like a browser wrapper
- the user has to click too much to get the answer
- the summary is visually pretty but decision-poor
- Copilot support pretends confidence it does not have
- the tray utility starts drifting into dashboard bloat

---

## 15. Release Readiness Checklist

### Product readiness
- [ ] tray icon behaves reliably on Windows
- [ ] left click opens compact panel reliably
- [ ] right click menu works reliably
- [ ] panel shows cached summary instantly
- [ ] on-open refresh works reliably
- [ ] periodic refresh works reliably
- [ ] manual refresh works reliably
- [ ] summary sorts by urgency correctly
- [ ] warning/critical thresholds behave correctly
- [ ] Codex detail view shows both 5h and weekly data
- [ ] Copilot detail view shows useful data or honest partial state
- [ ] stale/unavailable states are visually unambiguous
- [ ] `Esc` dismiss works predictably

### Experience readiness
- [ ] feels faster than checking a browser/dashboard
- [ ] no major web-app feel
- [ ] no unnecessary settings or chrome creep
- [ ] summary is understandable in seconds
- [ ] the app is trustworthy enough for daily use

---

## 16. MVP Summary

Usage Radar MVP is not trying to become an AI operations dashboard.

It is trying to become the fastest daily place to check AI usage pressure:
- tray-first
- fast-glance first
- honest about confidence
- native-feeling
- small and dependable
- focused on Codex first, then Copilot
