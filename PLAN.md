# Action Item Tracker - Planning Document

## Overview

Web-based action item tracker for Cosmic Frontier, replacing the current spreadsheet-based workflow. Hosted at `tracker.cosmicfrontier.org`.

## Data Model

### Trackers (Projects)

Each tracker represents a vendor/partner relationship with its own prefix:

| Field | Type | Notes |
|-------|------|-------|
| `id` | int | Auto-incrementing |
| `prefix` | string | e.g. "AD" (Astro Digital), "SP" (SpaceX), etc. |
| `name` | string | Full name, e.g. "Astro Digital" |
| `description` | text | Optional notes about the tracker |
| `next_number` | int | Next action item number for this tracker |

### Users

Org members who can create/own action items (synced from Google OAuth):

| Field | Type | Notes |
|-------|------|-------|
| `id` | int | Auto-incrementing |
| `email` | string | Google account email (unique) |
| `name` | string | Display name |
| `initials` | string | For note attribution (e.g. "MF") |
| `created_at` | timestamp | When user first logged in |

### Action Items

| Field | Type | Notes |
|-------|------|-------|
| `id` | string | Format: `{PREFIX}-XXX` (e.g. AD-001, SP-042) |
| `tracker_id` | int | Foreign key to tracker |
| `title` | string | Brief description |
| `create_date` | date | When item was created |
| `created_by_id` | int | FK to users - immutable after creation |
| `due_date` | date (nullable) | Target completion date, `NULL` = TBD |
| `category` | enum | See categories below |
| `owner_id` | int | FK to users - can be reassigned |
| `priority` | enum | High / Medium / Low |

### Categories
- Programmatic
- SW / Ops
- Mechanical
- ADCS
- Systems
- ConOps

### Statuses
- New
- Not Started
- In Progress
- TBC (To Be Confirmed)
- Complete
- Blocked

### Status Transitions

Status is derived from the `status_history` table - the current status is the most recent entry. Any transition is allowed; all transitions are logged for audit.

| Field | Type | Notes |
|-------|------|-------|
| `id` | int | Auto-incrementing |
| `action_item_id` | string | FK to action_items |
| `status` | enum | The new status |
| `changed_by_id` | int | FK to users |
| `changed_at` | timestamp | When the transition occurred |
| `comment` | text (nullable) | Optional reason for change |

## Features

### MVP (v1)

- [ ] **Tracker management**
  - [ ] View list of trackers (e.g. AD, SP, RL)
  - [ ] Create new tracker (prefix + name)
  - [ ] Edit tracker details
- [ ] **Action items**
  - [ ] View all action items in a table (filterable by tracker)
  - [ ] Filter by: tracker, category, owner, priority, status
  - [ ] Sort by any column
  - [ ] Create new action item (auto-assigns next ID for selected tracker)
  - [ ] Edit existing action item
  - [ ] Add timestamped notes to an item
- [ ] **Authentication**
  - [ ] Google OAuth login (shared with BookStack)
  - [ ] JWT-based session management
  - [ ] Protected API endpoints

### Future (v2+)

- [ ] Email/Slack notifications for due dates
- [ ] Audit log / history view
- [ ] Dashboard with metrics (items by status, overdue count, etc.)
- [ ] Linking between related items
- [ ] File attachments
- [ ] Comments / discussion threads

## Tech Stack

**Rust full-stack application:**

- **Frontend:** Yew (Rust WASM framework)
- **Backend:** Axum (async web framework)
- **Database:** diesel-async with PostgreSQL (NeonDB)
- **Architecture:** Single binary serving embedded WASM frontend + API

### Why This Stack
- Single language (Rust) for frontend and backend
- Type safety across the entire stack
- Yew compiles to WASM, embedded in the Axum binary via `rust-embed`
- `diesel-async` provides compile-time SQL verification with native async support
- NeonDB gives serverless Postgres with automatic scaling

### Frontend Embedding (rust-embed)

The Yew frontend is built with Trunk, then embedded into the backend binary using `rust-embed`:

```rust
// backend/src/static_files.rs
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../frontend/dist/"]
struct Assets;
```

The backend serves these with a fallback handler for SPA routing:
- `/api/*` → API routes (protected by JWT)
- `/auth/*` → OAuth flow (login, callback, logout)
- `/health` → Health check (public)
- `/*` → Static files from rust-embed, with `index.html` fallback for client-side routing

### Shared Crate

The `shared` crate contains API types and an HTTP client that compiles for both WASM and native targets:

```toml
# shared/Cargo.toml
[features]
default = []
wasm = ["gloo-net"]
native = ["reqwest"]

[dependencies]
serde = { version = "1", features = ["derive"] }
thiserror = "1"

[target.'cfg(target_arch = "wasm32")'.dependencies]
gloo-net = { version = "0.5", optional = true }

[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
reqwest = { version = "0.11", features = ["json"], optional = true }
```

```rust
// shared/src/client.rs
pub struct TrackerClient {
    base_url: String,
}

impl TrackerClient {
    pub fn new(base_url: &str) -> Self { ... }

    // These methods use gloo-net on WASM, reqwest on native
    pub async fn list_trackers(&self) -> Result<Vec<Tracker>, ClientError> { ... }
    pub async fn get_tracker(&self, id: i32) -> Result<Tracker, ClientError> { ... }
    pub async fn create_action_item(&self, item: NewActionItem) -> Result<ActionItem, ClientError> { ... }
    // etc.
}
```

**Usage:**
- Frontend (Yew): `shared = { path = "../shared", features = ["wasm"] }`
- Native tools: `shared = { path = "../shared", features = ["native"] }`

## Database Schema (PostgreSQL / NeonDB)

```sql
-- Trackers represent vendor/partner projects with unique prefixes
CREATE TABLE trackers (
    id SERIAL PRIMARY KEY,
    prefix VARCHAR(10) UNIQUE NOT NULL,  -- "AD", "SP", "RL", etc.
    name VARCHAR(255) NOT NULL,          -- "Astro Digital", "SpaceX", etc.
    description TEXT,
    next_number INTEGER DEFAULT 1,       -- auto-increment per tracker
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Users (created on first OAuth login)
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    initials VARCHAR(10),                -- For note attribution (e.g. "MF")
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE action_items (
    id VARCHAR(20) PRIMARY KEY,          -- AD-001, SP-042, etc.
    tracker_id INTEGER NOT NULL REFERENCES trackers(id),
    number INTEGER NOT NULL,             -- 1, 2, 3... (per tracker)
    title VARCHAR(500) NOT NULL,
    create_date DATE NOT NULL,
    created_by_id INTEGER NOT NULL REFERENCES users(id),
    due_date DATE,                       -- nullable (NULL = TBD)
    category VARCHAR(50) NOT NULL,
    owner_id INTEGER NOT NULL REFERENCES users(id),
    priority VARCHAR(20) NOT NULL,       -- High, Medium, Low
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(tracker_id, number)
);

-- Status history (current status = most recent entry per item)
CREATE TABLE status_history (
    id SERIAL PRIMARY KEY,
    action_item_id VARCHAR(20) NOT NULL REFERENCES action_items(id),
    status VARCHAR(50) NOT NULL,
    changed_by_id INTEGER NOT NULL REFERENCES users(id),
    changed_at TIMESTAMPTZ DEFAULT NOW(),
    comment TEXT
);

-- Notes are structured: each note has a date, author, and content
CREATE TABLE notes (
    id SERIAL PRIMARY KEY,
    action_item_id VARCHAR(20) NOT NULL REFERENCES action_items(id),
    note_date DATE NOT NULL,             -- Date the note refers to (user-selectable, defaults to today)
    author_id INTEGER NOT NULL REFERENCES users(id),
    content TEXT NOT NULL,               -- 1-10000 characters
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX idx_action_items_tracker ON action_items(tracker_id);
CREATE INDEX idx_action_items_owner ON action_items(owner_id);
CREATE INDEX idx_action_items_created_by ON action_items(created_by_id);
CREATE INDEX idx_action_items_due_date ON action_items(due_date);
CREATE INDEX idx_notes_action_item ON notes(action_item_id);
CREATE INDEX idx_notes_author ON notes(author_id);
CREATE INDEX idx_status_history_item ON status_history(action_item_id);
CREATE INDEX idx_status_history_changed_at ON status_history(action_item_id, changed_at DESC);
```

### Migration Naming Convention

Migrations live in `migrations/` and follow Diesel's standard format:

```
migrations/
├── 00000000000000_diesel_initial_setup/
│   └── up.sql, down.sql
├── 2026-02-03-000001_create_trackers/
│   └── up.sql, down.sql
├── 2026-02-03-000002_create_users/
│   └── up.sql, down.sql
├── 2026-02-03-000003_create_action_items/
│   └── up.sql, down.sql
├── 2026-02-03-000004_create_notes/
│   └── up.sql, down.sql
├── 2026-02-03-000005_create_status_history/
│   └── up.sql, down.sql
└── 2026-02-03-000006_create_indexes/
    └── up.sql, down.sql
```

**Format:** `YYYY-MM-DD-NNNNNN_description`
- Date prefix ensures chronological ordering
- 6-digit sequence number for multiple migrations on the same day
- Snake_case description: `create_<table>`, `add_<column>_to_<table>`, `drop_<table>`, etc.

**Examples:**
- `2026-02-10-000001_add_archived_to_trackers`
- `2026-02-15-000001_add_due_date_index`
- `2026-03-01-000001_create_attachments`

## UI Wireframes

### Tracker List View
```
+------------------------------------------------------------------+
| Action Item Tracker                          [+ New Tracker]      |
+------------------------------------------------------------------+
| Prefix | Name              | Open Items | Last Updated            |
|--------|-------------------|------------|-------------------------|
| AD     | Astro Digital     | 24         | 1/27/2026               |
| SP     | SpaceX            | 8          | 1/25/2026               |
| RL     | Rocket Lab        | 12         | 1/20/2026               |
+------------------------------------------------------------------+
```

### Action Items View (Per Tracker)
```
+------------------------------------------------------------------+
| AD - Astro Digital                                   [+ New Item] |
+------------------------------------------------------------------+
| Filters: [Category ▼] [Owner ▼] [Priority ▼] [Status ▼] [Search] |
+------------------------------------------------------------------+
| #      | Title          | Due    | Category | Owner | Pri | Status |
|--------|----------------|--------|----------|-------|-----|--------|
| AD-001 | Definitized... | 2/27   | Program  | M.F.  | Med | In Prog|
| AD-002 | Payload Task...| 3/31   | SW/Ops   | J.K.  | Med | TBC    |
| AD-006 | Integrated...  | TBD    | Program  | M.F.  | Med | In Prog|
+------------------------------------------------------------------+
```

### Detail / Edit View
```
+------------------------------------------------------------------+
| AD-001: Definitized Contract                      [Edit] [Back]   |
+------------------------------------------------------------------+
| Created: 1/16/2026 by M. Fitzgerald                               |
| Due: 2/27/2026                                                    |
| Category: Programmatic    Owner: M. Fitzgerald                    |
| Priority: Medium          Status: In Progress                     |
+------------------------------------------------------------------+
| Notes                                            [+ Add Note]     |
|------------------------------------------------------------------|
| 01/26/2026 MF: No Change                                         |
| 01/16/2026 MF: Comments received from CFL Legal...               |
+------------------------------------------------------------------+
```

## Deployment

- Docker container in `services/action-tracker/`
- Multi-stage Dockerfile: build Rust + WASM, then slim runtime image
- Traefik labels for `tracker.cosmicfrontier.org`
- Database: NeonDB PostgreSQL (connection string from secrets manager)
- Add to existing `docker-compose.yml`

### Docker Health Check

```dockerfile
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1
```

| Parameter | Value | Notes |
|-----------|-------|-------|
| `interval` | 30s | Check every 30 seconds |
| `timeout` | 5s | Fail if no response in 5 seconds |
| `start-period` | 10s | Grace period for container startup |
| `retries` | 3 | Mark unhealthy after 3 consecutive failures |

The `/health` endpoint returns:
```json
{"status": "ok"}
```

Returns `500` if database connection fails.

### Environment Variables

| Variable | Description | Example |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgres://user:pass@host/db` |
| `JWT_SECRET` | Secret key for signing/verifying JWTs | Random 32+ byte string |
| `ALLOWED_EMAIL_DOMAINS` | Comma-separated list of allowed email domains for OAuth | `cosmicfrontier.org,contractor.com` |
| `GOOGLE_CLIENT_ID` | Google OAuth client ID | From secrets manager |
| `GOOGLE_CLIENT_SECRET` | Google OAuth client secret | From secrets manager |
| `PUBLIC_URL` | Public URL for OAuth callbacks | `https://tracker.cosmicfrontier.org` |
| `PORT` | Server port (optional, default 8080) | `8080` |

## Authentication

Uses Google OAuth. The same OAuth client can be shared across multiple services on the same domain.

### Flow
1. User hits the app → redirected to Google OAuth
2. On successful auth, backend issues a JWT containing user info (email, name)
3. JWT stored in browser (cookie or localStorage)
4. All data-providing API endpoints validate the JWT before responding

### JWT Details
- **Expiration:** 24 hours (86400 seconds)
- **Storage:** httpOnly cookie named `token`
- **Algorithm:** HS256

```json
{
  "sub": "user@cosmicfrontier.org",
  "name": "M. Fitzgerald",
  "user_id": 42,
  "exp": 1234567890,
  "iat": 1234567890
}
```

### Protected Endpoints
All `/api/*` routes require valid JWT in `Authorization: Bearer <token>` header. Public routes (OAuth callback, health check) are excluded.

## API Routes

All `/api/*` routes require valid JWT. No pagination - all items returned (dataset is small).

### Auth Routes
| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/auth/login` | No | Redirect to Google OAuth |
| GET | `/auth/callback` | No | OAuth callback, sets JWT cookie |
| POST | `/auth/logout` | No | Clear JWT cookie |
| GET | `/auth/me` | Yes | Get current user info |

### Tracker Routes
| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/trackers` | Yes | List all trackers with open/total item counts |
| POST | `/api/trackers` | Yes | Create new tracker |
| GET | `/api/trackers/:id` | Yes | Get tracker details |
| PATCH | `/api/trackers/:id` | Yes | Update tracker (name, description) |

### Action Item Routes
| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/items` | Yes | List ALL items across all trackers |
| GET | `/api/trackers/:id/items` | Yes | List items for a specific tracker |
| POST | `/api/trackers/:id/items` | Yes | Create item (auto-assigns next ID) |
| GET | `/api/items/:item_id` | Yes | Get item details with notes |
| PATCH | `/api/items/:item_id` | Yes | Update item fields |

### Note Routes
| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/items/:item_id/notes` | Yes | List notes for an item |
| POST | `/api/items/:item_id/notes` | Yes | Add note to item |

### Status History Routes
| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/items/:item_id/history` | Yes | List status transitions for an item |
| POST | `/api/items/:item_id/status` | Yes | Change item status (creates history entry) |

### User Routes
| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/users` | Yes | List all users (for owner dropdown) |

### Health Check
| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | No | Returns `{"status": "ok"}` |

### Query Parameters

Items endpoints support client-side filtering (all data returned, filter in frontend), but these server-side filters are available:

| Endpoint | Parameter | Example |
|----------|-----------|---------|
| `/api/items` | `tracker_id` | `?tracker_id=1` |
| `/api/items`, `/api/trackers/:id/items` | `status` | `?status=In+Progress` |
| `/api/items`, `/api/trackers/:id/items` | `owner_id` | `?owner_id=5` |
| `/api/items`, `/api/trackers/:id/items` | `category` | `?category=Programmatic` |
| `/api/items`, `/api/trackers/:id/items` | `priority` | `?priority=High` |

## Frontend Routes (Yew)

| Path | View | Description |
|------|------|-------------|
| `/` | Tracker List | Home page, shows all trackers |
| `/trackers/:id` | Tracker Items | Items table for a specific tracker |
| `/items/:item_id` | Item Detail | View/edit single item with notes |
| `/items/:item_id/edit` | Item Edit | Edit form for item |
| `/trackers/new` | New Tracker | Create tracker form |
| `/trackers/:id/items/new` | New Item | Create item form |

### Deep Links (Shareable URLs)

Items can be linked directly by their ID:
- `https://tracker.cosmicfrontier.org/items/AD-001` → Opens item AD-001 detail view

The backend also supports a convenience redirect:
| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/go/:item_id` | No | Redirects to `/items/:item_id` (for short shareable links) |

Example: `https://tracker.cosmicfrontier.org/go/AD-001` redirects to the item detail page.

If the user is not authenticated, they are redirected to login first, then back to the item.

### View State Persistence (localStorage)

The frontend persists user view preferences in `localStorage`:

```typescript
// Key: "action-tracker-view-state"
{
  "version": 1,                    // Schema version - increment on breaking changes
  "lastTrackerId": 3,              // Last viewed tracker
  "filters": {
    "status": ["In Progress", "Blocked"],
    "category": null,
    "owner_id": null,
    "priority": null
  },
  "sort": {
    "column": "due_date",
    "direction": "asc"
  }
}
```

**Schema versioning:**
- `version` field tracks the localStorage schema version
- On app load, if stored `version` < current app version, clear and reset to defaults
- Prevents stale/incompatible state from causing errors after updates

**Stored preferences:**
- `lastTrackerId` - Redirects `/` to last viewed tracker (if set)
- `filters` - Active filter selections (null = show all)
- `sort` - Column and direction for item table

## API Conventions

### Error Response Format
```json
{
  "error": {
    "code": "NOT_FOUND",
    "message": "Action item AD-999 not found"
  }
}
```

| HTTP Status | Code | When |
|-------------|------|------|
| 400 | `VALIDATION_ERROR` | Invalid input (missing required field, bad format) |
| 401 | `UNAUTHORIZED` | Missing or invalid JWT |
| 403 | `FORBIDDEN` | Valid JWT but email domain not allowed |
| 404 | `NOT_FOUND` | Resource doesn't exist |
| 409 | `CONFLICT` | Duplicate (e.g., tracker prefix already exists) |
| 500 | `INTERNAL_ERROR` | Server error |

### Validation Rules

| Field | Rules |
|-------|-------|
| `tracker.prefix` | 2-5 uppercase letters, unique |
| `tracker.name` | 1-255 characters, required |
| `action_item.title` | 1-500 characters, required |
| `action_item.category` | Must be one of defined categories |
| `action_item.priority` | Must be `High`, `Medium`, or `Low` |
| `action_item.status` | Must be one of defined statuses |
| `note.content` | 1-10000 characters, required |

### Default Sort Order

| Endpoint | Default Sort |
|----------|--------------|
| `/api/trackers` | `prefix ASC` |
| `/api/items`, `/api/trackers/:id/items` | `id ASC` (AD-001, AD-002...) |
| `/api/items/:id/notes` | `note_date DESC, created_at DESC` (newest first) |
| `/api/items/:id/history` | `changed_at DESC` (newest first) |

## Project Structure

```
services/action-tracker/
├── Cargo.toml              # Workspace root
├── Dockerfile
├── frontend/               # Yew WASM app
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── components/
│       └── pages/
├── backend/                # Axum server
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── routes/
│       ├── models/
│       └── db/
├── shared/                 # Shared types + API client
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── types.rs        # Serde types (Tracker, ActionItem, Note, etc.)
│       └── client.rs       # HTTP client (feature-gated)
├── cli/                    # Admin CLI tools
│   ├── Cargo.toml
│   └── src/
│       └── main.rs         # Subcommands: migrate, create-user, reset-sequence, etc.
└── migrations/             # Diesel migrations
```

## CLI Tools

Minimal admin CLI using the shared native client:

```bash
# Run migrations
action-tracker-cli migrate

# Create a user manually (for seeding)
action-tracker-cli create-user --email "admin@cosmicfrontier.org" --name "Admin" --initials "ADM"

# Reset tracker sequence (if IDs get out of sync)
action-tracker-cli reset-sequence --tracker AD

# List users
action-tracker-cli list-users
```

The CLI reads `DATABASE_URL` from environment (or `.env` file) for direct DB access.

## CI/CD (GitHub Actions)

### Workflow: `.github/workflows/action-tracker.yml`

Triggers on push/PR to `main` when `services/action-tracker/**` changes.

```yaml
name: Action Tracker CI

on:
  push:
    branches: [main]
    paths: ['services/action-tracker/**']
  pull_request:
    branches: [main]
    paths: ['services/action-tracker/**']

defaults:
  run:
    working-directory: services/action-tracker

jobs:
  fmt:
    name: Format
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt
      - run: cargo fmt --all -- --check

  clippy:
    name: Clippy
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: services/action-tracker
      - run: cargo clippy --all-targets --all-features -- -D warnings

  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: services/action-tracker
      - run: cargo test --all

  build-backend:
    name: Build Backend
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: services/action-tracker
      - run: cargo build --release -p backend

  build-wasm:
    name: Build WASM Frontend
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: wasm32-unknown-unknown
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: services/action-tracker
      - name: Install Trunk
        run: cargo install trunk
      - name: Build frontend
        run: cd frontend && trunk build --release

  build-cli:
    name: Build CLI
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
        with:
          workspaces: services/action-tracker
      - run: cargo build --release -p cli
```

### CI Checks Summary

| Job | What it checks |
|-----|----------------|
| `fmt` | Code formatting (`cargo fmt`) |
| `clippy` | Lints and warnings (`-D warnings` = fail on any warning) |
| `test` | Unit and integration tests |
| `build-backend` | Backend compiles for x86_64 |
| `build-wasm` | Frontend compiles to WASM via Trunk |
| `build-cli` | CLI tools compile for x86_64 |

## Next Steps

1. Set up NeonDB database and store connection string in secrets manager
2. Initialize Rust workspace with frontend/backend/shared crates
3. Set up Diesel migrations
4. Build shared types and client (with feature flags)
5. Build backend API (Axum + Diesel)
6. Build frontend (Yew)
7. Embed frontend in backend binary
8. Dockerize and deploy
