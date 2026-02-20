# Manifold Markets API Reference

Practical guide to the Manifold Markets API, distilled from two working Rust codebases.

**Base URL:** `https://api.manifold.markets/v0`
**WebSocket URL:** `wss://api.manifold.markets/ws`
**Rate limit:** 500 requests/minute per IP

---

## Authentication

Most read endpoints are public. Write endpoints require an API key.

```
Authorization: Key <your-api-key>
```

Get your key from your Manifold profile settings. The `/me` endpoint is handy for validating a key:

```
GET /v0/me
Authorization: Key abc123
```

Returns the authenticated `User` object. If the key is invalid you get a non-2xx status.

---

## REST Endpoints

### Markets

#### GET /market/{id}

Fetch a single market by ID. **No auth required.**

```
GET /v0/market/abc123
```

Response fields (partial):

| Field | Type | Notes |
|---|---|---|
| `id` | string | Unique market ID |
| `question` | string | Market title |
| `url` | string | Full URL to market page |
| `probability` | float? | Current YES probability (0.0–1.0), only for BINARY |
| `outcomeType` | string | `BINARY`, `MULTIPLE_CHOICE`, `FREE_RESPONSE`, `NUMERIC`, etc. |
| `mechanism` | string | `cpmm-1` for most markets |
| `isResolved` | bool | Whether the market has resolved |
| `closeTime` | u64? | Unix timestamp in **milliseconds** |
| `creatorUsername` | string | Username of creator |
| `volume` | float | Total trading volume |
| `volume24Hours` | float | 24h trading volume |
| `uniqueBettorCount` | int | Number of unique bettors |
| `resolution` | string? | `YES`, `NO`, `MKT`, `CANCEL` (only if resolved) |
| `textDescription` | string | Plain-text description |

#### GET /markets

Paginated list of markets. **No auth required.**

| Parameter | Type | Notes |
|---|---|---|
| `limit` | int | Max 1000 |
| `sort` | string | `created-time`, `updated-time`, `last-bet-time`, `last-comment-time` |
| `order` | string | `asc` or `desc` |
| `before` | string | Market ID for cursor-based pagination |
| `userId` | string | Filter by creator ID |
| `groupId` | string | Filter by topic/group |

Returns `Vec<LiteMarket>` (a lighter version of the full market object).

#### GET /search-markets

Full-text search with filters. **No auth required.**

| Parameter | Type | Notes |
|---|---|---|
| `term` | string | Search query |
| `sort` | string | `most-popular`, `newest`, `score` |
| `filter` | string | `open`, `closed`, `resolved`, `news` |
| `contractType` | string | `BINARY`, `MULTIPLE_CHOICE`, etc. |
| `topicSlug` | string | Topic/group slug |
| `creatorId` | string | Filter by creator |
| `limit` | int | Max 1000 |
| `offset` | int | For offset-based pagination |

This is the go-to endpoint for bulk fetching. Use `sort=most-popular&filter=open` to get active, liquid markets.

### Users

#### GET /user/{username}

Fetch user by username. **No auth required.**

#### GET /user/by-id/{id}

Fetch user by ID. **No auth required.**

Key response fields: `id`, `username`, `name`, `balance`, `bio`, `avatarUrl`, `isBot`, `isAdmin`, `totalDeposits`, `lastBetTime`, `currentBettingStreak`.

### Bets

#### GET /bets

Fetch bets for a market. **No auth required.**

| Parameter | Type | Notes |
|---|---|---|
| `contractId` | string | **Required.** Market ID |
| `limit` | int | Max 1000 |
| `before` | string | Bet ID for cursor pagination |

Response fields:

| Field | Type | Notes |
|---|---|---|
| `id` | string | Bet ID |
| `contractId` | string | Market ID |
| `userId` | string? | Bettor's user ID |
| `amount` | float? | Mana wagered |
| `shares` | float? | Shares received |
| `outcome` | string? | `YES` or `NO` |
| `probBefore` | float | Probability before this bet |
| `probAfter` | float | Probability after this bet |
| `createdTime` | u64 | Unix timestamp (milliseconds) |
| `isFilled` | bool? | For limit orders |
| `isCancelled` | bool? | For limit orders |
| `limitProb` | float? | Limit order trigger probability |
| `orderAmount` | float? | Limit order total amount |

#### POST /bet

Place a bet. **Auth required.**

```json
{
  "contractId": "market-id-here",
  "amount": 10,
  "outcome": "YES",
  "limitProb": 0.60
}
```

- `contractId` — the market to bet on
- `amount` — mana to wager
- `outcome` — `"YES"` or `"NO"` for binary markets
- `limitProb` — optional, creates a limit order at this probability (0.0–1.0)

Returns `{ "betId", "amount", "outcome", "contractId" }`.

### Comments

#### GET /comments

Fetch comments for a market. **No auth required.**

| Parameter | Type | Notes |
|---|---|---|
| `contractId` | string | **Required.** Market ID |
| `limit` | int | Max 1000 |

Comments use TipTap JSON for rich text content. The `content` field is a nested document structure:

```json
{
  "type": "doc",
  "content": [
    {
      "type": "paragraph",
      "content": [
        { "type": "text", "text": "Hello world" }
      ]
    }
  ]
}
```

To extract plain text, recursively walk the content tree and collect all `text` fields.

---

## WebSocket API

The WebSocket API pushes real-time events. Connect to `wss://api.manifold.markets/ws`. No auth required.

### Protocol

All messages are JSON. Three message types:

**Client → Server:**

```json
{ "type": "subscribe", "txid": 1, "topics": ["global/new-contract"] }
{ "type": "unsubscribe", "txid": 2, "topics": ["global/new-contract"] }
{ "type": "ping", "txid": 3 }
```

`txid` is a client-chosen integer for correlating acks.

**Server → Client:**

```json
{ "type": "ack", "txid": 1, "success": true }
{ "type": "broadcast", "topic": "global/new-contract", "data": { ... } }
```

### Available Topics

**Global topics:**
- `global/new-contract` — all new markets
- `global/new-bet` — all new bets across all markets
- `global/updated-contract` — updates to any public market
- `global/new-comment` — all new comments

**Per-market topics** (replace `{marketId}`):
- `contract/{marketId}` — general updates for a specific market
- `contract/{marketId}/new-bet` — new bets on a specific market
- `contract/{marketId}/new-comment` — new comments on a specific market

### `global/new-contract` Broadcast Shape

```json
{
  "contract": {
    "id": "...",
    "slug": "...",
    "question": "Will X happen?",
    "outcomeType": "BINARY",
    "mechanism": "cpmm-1",
    "visibility": "public",
    "createdTime": 1700000000000,
    "closeTime": 1700100000000,
    "isResolved": false,
    "probability": 0.5,
    "p": 0.5,
    "totalLiquidity": 100.0,
    "volume": 0.0
  },
  "creator": {
    "id": "...",
    "username": "alice",
    "name": "Alice"
  }
}
```

### Connection Management

Manifold's WebSocket requires active keepalive:

- **Send a ping every 30 seconds** (WebSocket protocol ping frame or a JSON `{ "type": "ping", "txid": N }`)
- **Auto-reconnect on disconnect** with a 3-second delay
- **Health checks:** Consider a connection stale if:
  - No ack received within 120 seconds of a subscribe
  - A ping goes unacknowledged for 60 seconds
  - No messages received for 5 minutes

---

## Practical Patterns

### Pagination

Two pagination styles depending on endpoint:

**Offset-based** (`/search-markets`): Use `limit` + `offset`. Max 1000 per request. Increment offset by the number of results returned. Stop when you get fewer results than `limit`.

**Cursor-based** (`/markets`, `/bets`): Use `limit` + `before` (pass the ID of the last item received). Stop when you get an empty result.

### Rate Limiting

The API allows 500 requests/minute. To stay safe:

- Add **50ms delays** between individual detail requests (e.g., fetching full market data one by one)
- Add **100ms delays** between pagination batches
- This keeps you well under the limit even during bulk scraping

### Retry Strategy

Server errors (502, 503, 504) are common during high load. Use exponential backoff:

```
Attempt 1: wait 1s
Attempt 2: wait 2s
Attempt 3: wait 4s
Attempt 4: wait 8s
Attempt 5: wait 16s
Give up after 5 attempts.
```

Only retry on 502/503/504. Don't retry on 4xx errors (those indicate a real problem with your request).

### Filtering for Tradeable Markets

Most bot use cases only care about binary markets that are still open:

1. Check `outcomeType == "BINARY"` — only these have a single probability value
2. Check `isResolved == false`
3. Check `mechanism == "cpmm-1"` — the standard automated market maker
4. Optionally check `closeTime > now` to exclude markets about to close
5. Check `visibility == "public"` when using WebSocket data

### Timestamps

All timestamps in the API are **Unix milliseconds** (not seconds). Divide by 1000 to get standard Unix time.

```rust
// Rust example
let secs = timestamp_ms / 1000;
let nanos = (timestamp_ms % 1000) * 1_000_000;
NaiveDateTime::from_timestamp_opt(secs as i64, nanos as u32)
```

### Limit Orders

To place a limit order instead of a market order, include `limitProb` in the bet request:

```json
{
  "contractId": "...",
  "amount": 50,
  "outcome": "YES",
  "limitProb": 0.40
}
```

This creates a standing order to buy YES shares if/when the price drops to 40%. The `amount` is the maximum mana to spend. Unfilled portions remain as open orders.

---

## JSON Field Naming

The API uses **camelCase** for all field names. When deserializing in Rust, use `#[serde(rename_all = "camelCase")]` on your structs.

```rust
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub id: String,
    pub outcome_type: String,  // maps to "outcomeType" in JSON
    pub close_time: Option<u64>,
}
```

---

## Common Pitfalls

1. **Timestamps are milliseconds**, not seconds. Forgetting this gives you dates in the year 50000+.
2. **`probability` is nullable.** Non-binary markets (multiple choice, free response) don't have a single probability field.
3. **Limit of 1000 per request.** If you need more, you must paginate.
4. **Comments are TipTap JSON**, not plain text. You need to extract text recursively.
5. **WebSocket requires keepalive.** Without pings every 30s, the server will drop your connection silently.
6. **`/search-markets` and `/markets` are different endpoints** with different parameter sets and pagination styles. Use `/search-markets` for filtering by popularity or search terms; use `/markets` for chronological listing.
