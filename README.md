# manifold-robot

Desktop trading bot for [Manifold Markets](https://manifold.markets). Monitors new markets via WebSocket, researches them with xAI's Grok, and makes trading decisions based on probability discrepancies.

Built with Rust, [Dioxus 0.7](https://dioxuslabs.com) (desktop webview), and Tailwind CSS.

## How it works

1. Connects to Manifold's WebSocket feed for new markets
2. Filters for binary (YES/NO) markets
3. Sends each market question to xAI for research (with web search)
4. Compares xAI's probability estimate to the current market price
5. Logs trade signals when a significant edge is found

## Setup

```bash
cp .env.example .env
```

Fill in your API keys:

```
MANIFOLD_API_KEY=your-manifold-key
XAI_API_KEY=your-xai-key
```

Get your Manifold key from your [profile settings](https://manifold.markets/profile). Get an xAI key from [x.ai](https://x.ai).

## Running

```bash
dx serve
```

Requires the [Dioxus CLI](https://dioxuslabs.com/learn/0.7/getting_started):

```bash
curl -sSL http://dioxus.dev/install.sh | sh
```

## Architecture

```
src/
├── main.rs  # Dioxus UI, app state, dashboard
├── api.rs   # Manifold Markets REST client
├── bot.rs   # Trading bot logic
├── ws.rs    # WebSocket client (market feed)
└── xai.rs   # xAI/Grok research client
```
