# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Desktop-only Rust application for a **Manifold Markets trading bot**. Uses **Dioxus 0.7** for the UI (native desktop window via webview). This is not a fullstack/web app — it's a single desktop process that provides a UI for configuring and monitoring the bot.

## Build & Serve Commands

```bash
dx serve                      # Serve the desktop app (hot-reloading)
cargo clippy                  # Lint
```

The `dx` CLI is the Dioxus build tool. Tailwind CSS compilation is automatic in Dioxus 0.7+ via the `tailwind.css` file in the project root.

## Linting

The `clippy.toml` enforces Dioxus-specific rules: never hold `GenerationalRef`, `GenerationalRefMut`, or `WriteLock` across await points. This prevents borrow conflicts in async code with signals.

## Dioxus 0.7 Key Patterns

**AGENTS.md contains a comprehensive Dioxus 0.7 API reference.** Always consult it — Dioxus 0.7 has breaking changes from earlier versions. Notably: `cx`, `Scope`, and `use_state` no longer exist.

Key patterns:
- Components: functions annotated with `#[component]`, returning `Element`
- Props must be owned (`String` not `&str`), implement `PartialEq + Clone`
- State: `use_signal` for local state, `use_memo` for derived values
- Assets: `asset!("/assets/filename")` macro for referencing files (paths relative to project root)
- RSX: use `for` loops and `if` directly in `rsx!{}` — prefer loops over iterators
- Context: `use_context_provider` / `use_context` for sharing state down the tree
- Async: `use_resource` for async operations

## Architecture

### Module Overview

- **`main.rs`** — Dioxus UI components and app orchestration. Root `App` component manages authentication state, spawns background tasks, and provides all shared signals via `use_context_provider`.
- **`api.rs`** — `ManifoldClient` HTTP wrapper for Manifold Markets REST API (`/v0`). Auth via `Authorization: Key <key>` header.
- **`bot.rs`** — Trading bot logic. Listens for WebSocket market events, filters for BINARY markets, spawns xAI research tasks, and logs trade decisions.
- **`ws.rs`** — WebSocket client connecting to `wss://api.manifold.markets/ws`. Subscribes to `global/new-contract` topic. Auto-reconnects every 3s, pings every 30s.
- **`xai.rs`** — `XaiClient` for xAI's Grok API (`grok-4-1-fast` model). Uses `x_search` and `web_search` tools. Parses structured `PROBABILITY: XX%` / `REASONING:` responses.

### Data Flow

1. User enters Manifold + xAI API keys → validated via `ManifoldClient::get_me()`
2. `BotDashboard` spawns WebSocket connection and bot task, bridges them with `mpsc::unbounded_channel`
3. WebSocket broadcasts new markets → bot researches via xAI → logs trade decisions
4. UI receives events via `tokio::select!` multiplexing two channels (ws events + bot logs) and updates signals

### State Management

All shared state lives as `Signal<T>` in the root `App` component, provided via `use_context_provider`. Child components consume via `use_context::<Signal<T>>()`. Key signals: `api_key`, `xai_key`, `user_info`, `connection_status`, `log_entries`, `ws_events`.

### Concurrency

- `tokio::spawn()` for background tasks
- `mpsc::unbounded_channel` for inter-task communication
- `tokio::select!` for multiplexing async streams
- Never hold signal refs across `.await` (enforced by clippy)

### Environment Variables

Loaded automatically via `dotenvy::dotenv().ok()` at startup. Required: `MANIFOLD_API_KEY`, `XAI_API_KEY`. See `.env.example`.

## Styling

Dual CSS approach:
- `tailwind.css` (project root) — Tailwind input file, auto-compiled by `dx serve`
- `assets/main.css` — hand-written global styles (dark theme: bg `#0f1116`)
- Both injected via `document::Link` components in the `App` component
