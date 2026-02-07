# Focus Context Capture + Prompt Injection (Windows + macOS)

## Summary
Add a cross-platform, best-effort focus context snapshot in the Tauri backend. The backend continuously pushes the newest focus context to the TypeScript frontend. The frontend only sends focus context to the Python server when recording starts. The server injects focus context into the LLM system prompt at the start of each recording. Browser tab metadata is best-effort: Windows infers tab title from window title and attempts origin extraction via UIA for Chromium-family browsers plus Firefox.

## High-Level Flow
- [x] Tauri focus watcher emits `focus-context-changed` to the frontend.
- [x] Frontend maintains `latestFocusContext` in memory.
- [x] Frontend sends focus context in `start-recording` payload.
- [x] Server injects focus context into the system prompt for the recording.

## Public API / Interface Changes
- [x] `focus_get_current_context` Tauri command.
- [x] `focus-context-changed` event with `FocusContextSnapshot` payload.
- [x] `start-recording` carries optional `data.focus_context`.
- [x] Privacy setting `send_focus_context_enabled` with warning.

## Implementation Details (Design Decisions)
### Windows APIs
- Win32 window + process:
  - `GetForegroundWindow`, `GetWindowTextW`
  - `GetWindowThreadProcessId`, `OpenProcess`, `QueryFullProcessImageNameW`
- UI Automation:
  - `IUIAutomation` to traverse the accessibility tree (no screenshots)
  - Chromium family adapters first (Chrome, Edge, Brave, Opera), with best-effort Firefox support

### macOS APIs and Crates
- Crates:
  - `objc2`, `objc2-foundation`, `objc2-app-kit`, `objc2-accessibility`
  - `core-foundation`, `core-graphics`
- APIs:
  - `NSWorkspace.sharedWorkspace.frontmostApplication`
  - `AXUIElement`, `AXObserver` for focused window/tab

### Capability + Quality Model
- `FocusEventSource`: `polling | accessibility | uia | unknown`
- `FocusConfidenceLevel`: `high | medium | low`

### Design Decisions Made During Implementation
- Focus context is sent inside `start-recording` payload to avoid message ordering issues.
- Server stores focus context briefly to inject into the prompt for that recording only.
- Forward compatibility enforced with `extra="ignore"` on focus-related Pydantic models.
- Focus watcher emits only when semantic fields change, with debounce for noise reduction.
- Privacy control defaults to enabled, with a warning on disable.

## Implementation Checklist

### Shared Types
- [x] Rust `FocusContextSnapshot`, `FocusedApplication`, `FocusedWindow`, `FocusedBrowserTab`.
Focus context includes app, window, and tab fields plus `event_source`, `confidence_level`, `privacy_filtered`, and `captured_at`.
- [x] Rust `FocusEventSource`, `FocusConfidenceLevel`.
Event sources: `polling | accessibility | uia | unknown`. Confidence: `high | medium | low`.
- [x] TS `FocusContextSnapshot`.
Types live in `app/src/lib/focus.ts` and are re-used by events and APIs.
- [x] Server `FocusContextSnapshot` and related models.
Server accepts focus context on `start-recording` with forward-compatible parsing.

### Tauri Backend
- [x] `app/src-tauri/src/focus/mod.rs` module created.
- [x] Focus watcher loop with debounce and semantic dedupe.
Polls every 250ms, debounces 75ms, and dedupes on semantic key (app/window/tab + confidence).
- [x] `focus_get_current_context` command.
Returns a snapshot directly for diagnostics or on-demand use.
- [x] Watcher started in `.setup()` and emits `focus-context-changed`.
Event payload is `FocusContextSnapshot`.
- [x] `AppState` stores watcher handle.
Allows graceful shutdown and later extensions.

### Windows Focus Capture
- [x] Foreground window title via `GetForegroundWindow` + `GetWindowTextW`.
- [x] Process path via `GetWindowThreadProcessId` + `OpenProcess` + `QueryFullProcessImageNameW`.
- [x] App display name from process path.
- [x] Browser tab title inferred from window title.
Title inference trims `" - "` suffix when present.
- [x] UI Automation (UIA) for URL extraction.
Use `IUIAutomation` to locate Chromium-family address bar controls and read `ValuePattern` data.
- [x] URL privacy filtering (origin only).
Drop path, query parameters, and fragments before sending to the server.

### macOS Focus Capture
- [x] Frontmost app via `NSWorkspace.sharedWorkspace.frontmostApplication`.
- [x] Focused window title via Accessibility (`AXUIElement`).
Use AX focused window or focused UI element when available.
- [x] Browser tab title/URL via Accessibility tree.
Attempt Safari/Chrome/Edge/Brave/Firefox with AXURL and active tab title.
- [x] Accessibility permission check with low-confidence fallback.
If permission missing, return `privacy_filtered=true` with low confidence.

### Frontend
- [x] `focus-context-changed` event wiring in `tauriAPI`.
`tauriAPI.onFocusContextChanged` uses typed payloads.
- [x] Overlay listener updates `latestFocusContextRef`.
Ref is updated in a dedicated `useEffect`.
- [x] `start-recording` includes optional focus context payload.
Payload is gated by `send_focus_context_enabled`.
- [x] Privacy toggle in settings UI with warning.
Modal warns about dictation quality impact when disabling.

### Server
- [x] Parse `start-recording` with optional `data.focus_context`.
`StartRecordingData` and `FocusContextSnapshot` accept extra fields.
- [x] Store focus context on `DictationContextManager`.
Snapshot is stored per connection before `reset_context_for_new_recording`.
- [x] Prompt injection with unknown-context guard.
Injects when focus context is present, but skips injection when the snapshot is entirely unknown (no app/window/tab).
- [x] Forward-compat parsing for focus context data.
`ConfigDict(extra=\"ignore\")` used in Pydantic models.

### Settings Export/Import
- [x] `send_focus_context_enabled` stored in settings.
- [x] Export includes `send_focus_context_enabled`.
- [x] Import and factory reset include `send_focus_context_enabled`.

## Defaults
- [x] Focus poll interval: 250ms.
- [x] Debounce: 75ms.
- [x] `send_focus_context_enabled` default: true.

## Outstanding Work
- [ ] Harden Firefox/other non-Chromium URL extraction robustness on Windows across locales and browser versions.
- [ ] Improve browser tab title extraction on Windows by querying UIA tab controls where available instead of only window-title inference.
