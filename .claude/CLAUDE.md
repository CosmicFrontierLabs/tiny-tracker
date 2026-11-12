# Tiny Tracker

Action item tracker for small teams. Full-stack Rust: Yew/WASM frontend + Axum backend in a single binary.

## Architecture

```
tiny-tracker/
├── backend/         # Axum web server + API
├── frontend/        # Yew WASM SPA
├── shared/          # API types shared between frontend & backend
├── cli/             # Admin CLI (user/vendor management)
├── migrations/      # Diesel SQL migrations
├── Dockerfile       # Multi-stage build (frontend WASM → backend binary → slim runtime)
└── .github/workflows/ci.yml  # Build + push to GHCR on main
```

### How the build works

1. **Frontend**: Trunk compiles the Yew app to WASM → `frontend/dist/`
2. **Backend**: rust-embed bundles `frontend/dist/` into the Axum binary
3. **Runtime**: Single binary serves the SPA + API on port 8080

The Cargo workspace has four members: `backend`, `frontend`, `shared`, `cli`.

## Tech Stack

- **Frontend**: Yew (Rust → WASM), built with Trunk
- **Backend**: Axum (async), diesel-async with deadpool connection pool
- **Database**: PostgreSQL (NeonDB in production, local Docker for dev)
- **Auth**: Google OAuth2 → JWT in HttpOnly cookie (24h expiry)
- **TLS**: diesel-async uses rustls + tokio-postgres-rustls for NeonDB SSL
- **Container**: GHCR (`ghcr.io/cosmicfrontierlabs/tiny-tracker`)

## Development

```bash
# Start local Postgres + run migrations + seed dev data
./dev.sh start

# Run backend (serves API on :8080)
cargo run -p backend

# Run frontend with hot reload (proxies API to backend)
cd frontend && trunk serve

# Stop local Postgres
./dev.sh stop
```

Dev mode (`DEV_MODE=true`) bypasses OAuth and uses a local dev user.

## Key Patterns

### Database connections require TLS in production
`AsyncPgConnection::establish()` does not support TLS. The pool uses `ManagerConfig::custom_setup` with `tokio-postgres-rustls` to create TLS connections via `AsyncPgConnection::try_from_client_and_connection`. See `backend/src/main.rs`.

### Auth flow
1. Frontend checks `GET /auth/me` on load
2. If 401 → show login page with "Sign in with Google" link to `/auth/login`
3. `/auth/login` → Google OAuth → `/auth/callback` → creates/finds user in DB → sets JWT cookie → redirects to `/`
4. All `/api/*` routes extract `AuthUser` from JWT cookie via `FromRequestParts`

### Status is derived from history
Action items don't have a `status` column. Current status = most recent entry in `status_history` table. All transitions are logged.

### Action item IDs are composite
Format: `{VENDOR_PREFIX}-{NUMBER}` (e.g. `AD-001`). Generated server-side using the vendor's `next_number` counter.

### Vendor portal tokens are a separate credential family

Vendor contacts get a read-only view of one vendor's open items via an emailed
magic link. Three things keep that separate from staff access, and all three
matter:

1. **Different signing key.** Portal JWTs are signed with a key derived from
   `JWT_SECRET` (`vendor_jwt_secret()`), so a portal token cannot validate as a
   staff token or vice versa.
2. **Explicit scope claim.** `Claims.scope` must be `Staff` for `/api/*`, and
   `VendorClaims.scope` must be `VendorPortal` for `/vendor/api/*`. `scope`
   defaults to `Staff` when absent so cookies issued before the portal existed
   keep working.
3. **Cookie path.** The portal cookie is `vendor_token` scoped to `Path=/vendor`,
   so browsers never attach it to a staff request.

The emailed URL carries a single-use nonce, not the session token — only the
SHA-256 of the nonce is stored. `/vendor/verify` claims it with a conditional
`UPDATE ... RETURNING`, so concurrent clicks cannot both succeed. The
`VendorAuth` extractor reloads the vendor on every request and rejects archived
ones, so archiving a vendor cuts live sessions immediately.

### Mail transports are swappable and validated at startup

`MAIL_BACKEND` selects `smtp`, `resend`, or `log` behind the `Mailer` trait.
`mail::build` fails fast if the chosen backend's config is incomplete, so a
misconfiguration stops the container rather than silently dropping mail — note
this means missing mail config prevents startup entirely.

`Mailer::delivers()` is false for the `log` backend. Anything reporting success
to a human must check it, or a "sent" result is a lie. The staff Test Email
dialog (`POST /api/mail/test`) relies on this.

### Mail health has four states, and "untested" is one of them

A sending domain that is not yet verified produces a **perfectly healthy
container that rejects every message** — no crash, no restart, and with no
bounce webhook, no signal anywhere. That is worse than a crash-loop, because a
crash-loop is loud. Startup validation cannot catch it: the address is
syntactically fine, and a `GET /domains` probe would 401 for correctly scoped
send-only API keys.

So `MailHealthTracker` records the outcome of every send — both `POST
/api/mail/test` and the vendor portal's background send — and `GET
/api/mail/status` reports one of `not_delivering`, `untested`, `failing`, `ok`.
The staff UI banners the first three.

`untested` exists because a freshly deployed portal that has never attempted a
send is not healthy, it is unproven, and it must not render the same as one
known to work. Transport capability overrides attempt history, so the `log`
backend reports `not_delivering` no matter how many sends "succeed".

When changing mail config, **gate on a real send, never on the container coming
up.**

SMTP credentials are optional: Workspace's relay can authorise by static egress
IP, so omitting **both** `SMTP_USER` and `SMTP_PASSWORD` is a supported mode.
Setting exactly one is rejected as a typo.

The EHLO name is set explicitly, defaulting to the `MAIL_FROM` domain. Do not
rely on lettre's default — it is `hostname::get()`, which in a container is the
container ID, and Google's relay treats the EHLO domain as one of two ways it
identifies a sending domain. On the credential-less path there is no SMTP AUTH
to identify us instead, so a hex container ID risks a `550 5.7.1`.

### Admin is an env allowlist, checked per request

`ADMIN_EMAILS` is a comma-separated, case-insensitive list of emails permitted to
perform destructive actions — today that means deleting an action item. It is
**not** a database role and **not** a JWT claim: `AppConfig::is_admin` is
consulted on every request, so removing someone from the list revokes the
privilege at their next request rather than whenever their 24h token expires.

Enforcement is the `AdminUser` extractor in `backend/src/routes/mod.rs`. Naming
it in a handler signature is what makes a route admin-only, so there is no
in-handler check to forget. `/auth/me` reports `is_admin` purely so the frontend
can hide the control; the server never trusts it.

An empty or unset `ADMIN_EMAILS` admits nobody. Dev mode grants admin
unconditionally, since it already bypasses authentication entirely.

### Deleting an item is a soft delete

`DELETE /api/items/:id` sets `deleted_at` / `deleted_by_id` on the row. Nothing
is destroyed: the item, its notes and its full status history all stay, and the
ID stays claimed so it can never be reissued.

This is not politeness, it is the backup posture. Production has no
point-in-time recovery and only a nightly dump, so a destructive delete between
dumps would lose up to a day of history with no way back. A soft delete also
means the record of *what* was deleted is a database row that outlives the
container, rather than a log line that does not.

**Every read path must filter `deleted_at IS NULL`.** Miss one and a deleted
item reappears somewhere. The current set is `items::list_items_internal`,
`items::get`, `items::update` (filtering there is what stops an edit
resurrecting an item), `items::go_redirect`, `ensure_item_exists` in
`routes/mod.rs` (which covers every note and status route), the vendor item
counts in `vendors.rs`, both halves of the `activity.rs` UNION, and both queries
in the vendor portal.

Restoring is an admin CLI operation, deliberately not in the web UI:

```bash
cargo run -p cli -- list-deleted
cargo run -p cli -- restore-item --id AD-001
```

### Risk register: risks are the reasons, action items are the work

`risks` is a register of risks *about* vendors, numbered as one programme-wide
series: `RISK-034`, from the `risk_number_seq` sequence (atomic, so concurrent
creates cannot collide). Action items keep the vendor's prefix (`AD-001`)
because they are the vendor's work; a risk is ours and reads the same from any
view, so it carries no prefix. `vendor_id` records which vendor it concerns.
Action items map into risks many-to-many
through `action_item_risks`; one risk can be retired by several items and one
item can address several risks. There is no same-vendor constraint on links.
Links can be made from either side: one at a time from the item modal, or
several at once from the risk modal and at risk creation (`CreateRisk.item_ids`,
`POST /api/risks/:id/items`). Batch links check every ID before writing any
row, so a typo rejects the whole request rather than leaving a half-linked risk.

**Risks are staff-only.** Every risk route is under `/api/*` behind `AuthUser`,
the vendor portal's `/vendor/api/*` handlers never read the `risks` or
`action_item_risks` tables, and `VendorPortalItem` carries no risk fields.
Keep it that way: the register holds candid assessments of the vendor's own
hardware, and a vendor must not be able to learn which of their items we
consider a High.

**Score and level are never stored.** Score is severity × likelihood (each
0–5, half points allowed), level is High ≥ 15, Medium ≥ 8, else Low. Both live
in `shared::risk_score` / `shared::risk_level` so the API, the WASM UI and the
CLI importer cannot disagree. Storing them would only create a second place
for the truth to drift.

Status (`Open`, `In work`, `Mitigated`, `Accepted`) and trend (`New`,
`Improving`, `Stable`, `Worsening`, `Closed`) are plain columns with CHECK
constraints, not history-derived like item status. Trend is a human judgement
recorded at review; `Closed` is a *trend* rather than a status because that is
how the register has always been kept. The UI hides `Closed` risks by default.

`component` ("Item" in the UI and in the sheet) and `category` are free text
with datalist suggestions drawn from existing risks, not lookup tables: the
sets are small, vendor-specific and change as hardware is renamed.

Risks soft-delete exactly like items (admin-only, `deleted_at`), and **every
read path filters `deleted_at IS NULL`**: `risks::list_internal`,
`load_risk_response`, `update`, `ensure_risk_exists` (which covers linking
and the per-risk item list), and `risks_for_item`. The link-count subquery
must *also* filter deleted items, or a deleted item keeps propping up a count.

```bash
cargo run -p cli -- import-risks-csv --file register.csv --vendor NEU --dry-run
cargo run -p cli -- list-deleted-risks
cargo run -p cli -- restore-risk --id RISK-034
```

The importer takes a CSV export of the mastersheet (columns `ID, Item,
Category, Risk, Cause, Effect, Sev, Lik, Score, Level, Mitigation, Test type,
Verification (test IDs), Status, Opened, Updated, Trend`), preserves the
sheet's `R<n>` numbers as `RISK-<n>`, refuses duplicate IDs in the file,
refuses numbers already used by another vendor's risks, advances the sequence
past the highest imported number, and warns when the sheet's Level disagrees
with the computed one.

### Renaming a vendor prefix rewrites its item IDs

An action item's user-facing ID (`AD-001`) is also its primary key, and
notes, status history and risk links reference that string. Nothing else
links through the prefix: items and risks point at `vendors.id`. So a prefix
rename means rewriting the item IDs, and `PATCH /api/vendors/:id` with
`{"prefix": "NEW"}` does exactly that in one transaction: update the vendor,
then `UPDATE action_items SET id = new || substr(id, len+1)` for that vendor,
with the three child tables following via `ON UPDATE CASCADE` (migration
`000014`). Either everything renames or nothing does. Risk IDs (`RISK-034`)
carry no prefix and are untouched.

Old deep links (`/go/AD-001`, `/items/AD-001`) stop resolving after a rename;
there is no redirect table, because renames are meant to be rare and the
Manage Vendors UI says so before it lets you confirm.

### Vendor tracking IDs are free text, not a table

`action_items.vendor_tracking_ids` is a nullable `TEXT` holding the vendor's
own ticket references — keys like `VND-4521` or full URLs, comma-separated.
It is deliberately not a structured table or a per-vendor URL template: an
item maps to one or two external tickets, staff paste whatever the vendor
sent, and guessing a URL for a bare key would send people to the wrong place.
The server trims and folds blank input to `NULL`; the frontend splits on
commas, semicolons and newlines and only linkifies tokens that start with
`http(s)://`. The vendor portal shows it too, so the vendor can match our ID
to theirs.

### API responses use shared types, not `json!`
Backend route handlers must serialize responses using structs from the `shared` crate (e.g. `shared::Vendor`, `shared::VendorWithCounts`), not ad-hoc `serde_json::json!({})` objects. This keeps the frontend and backend type contracts in sync.

## API Routes

All `/api/*` routes require authentication (JWT cookie).

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| GET | `/auth/login` | Start OAuth flow |
| GET | `/auth/callback` | OAuth callback |
| POST | `/auth/logout` | Clear session |
| GET | `/auth/me` | Current user info |
| GET/POST | `/api/vendors` | List / create vendors |
| GET/PATCH | `/api/vendors/:id` | Get / update vendor (a `prefix` change rewrites its item IDs) |
| GET | `/api/items` | List all items |
| GET/POST | `/api/vendors/:id/items` | List / create items for vendor |
| GET/PATCH/DELETE | `/api/items/:id` | Get / update / soft-delete item (delete is admin-only) |
| GET/POST | `/api/items/:id/notes` | List / add notes |
| GET | `/api/items/:id/history` | Status history |
| POST | `/api/items/:id/status` | Change status |
| GET | `/api/users` | List users |
| POST | `/api/mail/test` | Send a test email (staff diagnostics) |
| GET | `/api/mail/status` | Mail transport health (staff banner) |
| GET | `/api/categories` | List all categories |
| GET/POST | `/api/vendors/:id/categories` | List / create categories for vendor |
| GET | `/go/:item_id` | Deep link redirect |
| GET | `/api/risks` | List risks (`?vendor_id=`, `?status=`), highest score first |
| GET/POST | `/api/vendors/:id/risks` | List / create risks for vendor |
| GET/PATCH/DELETE | `/api/risks/:id` | Get / update / soft-delete risk (delete is admin-only) |
| GET/POST | `/api/risks/:id/items` | Live action items linked to a risk / link several (`{"item_ids":[...]}`) |
| GET/POST | `/api/items/:id/risks` | Risks linked to an item / link one (`{"risk_id"}`) |
| DELETE | `/api/items/:id/risks/:risk_id` | Unlink |
| GET/POST | `/api/vendors/:id/allowed-domains` | List / add portal email domains |
| DELETE | `/api/vendors/:id/allowed-domains/:domain_id` | Revoke a portal domain |

### Vendor Portal Routes

Read-only, magic-link authenticated. Not under `/api/*` and never accept a staff token.

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/vendor/request-link` | none | Email a sign-in link |
| GET | `/vendor/verify` | nonce | Consume link, set portal cookie |
| POST | `/vendor/logout` | none | Clear portal cookie |
| GET | `/vendor/api/me` | portal | Current vendor + session expiry |
| GET | `/vendor/api/items` | portal | That vendor's open items |

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `DATABASE_URL` | Yes | PostgreSQL connection string |
| `JWT_SECRET` | Prod only | Secret for signing JWTs |
| `GOOGLE_CLIENT_ID` | Prod only | Google OAuth client ID |
| `GOOGLE_CLIENT_SECRET` | Prod only | Google OAuth client secret |
| `PUBLIC_URL` | Yes | Base URL for OAuth callbacks |
| `PORT` | No | Server port (default: 8080) |
| `ALLOWED_EMAIL_DOMAINS` | No | Comma-separated allowed domains |
| `ADMIN_EMAILS` | No | Comma-separated emails allowed to delete items. Empty = nobody |
| `DEV_MODE` | No | Set to `true` to bypass OAuth |
| `DEV_USER_ID` | No | User ID for dev mode |
| `MAIL_BACKEND` | No | `smtp` \| `resend` \| `log` (default `log` in dev, else `smtp`) |
| `MAIL_FROM` | Prod only | From header, e.g. `Tracker <notifications@tracker.example.org>` |
| `SMTP_HOST` / `SMTP_PORT` | No | Default `smtp.gmail.com` / `587` |
| `SMTP_USER` / `SMTP_PASSWORD` | No | Both for authenticated SMTP; **neither** for IP-authorised relay. Exactly one is a startup error |
| `SMTP_HELO_NAME` | No | EHLO domain; defaults to the `MAIL_FROM` domain |
| `RESEND_API_KEY` | If resend | Resend API key |

## Deployment

The CI workflow builds a Docker image and pushes to GHCR on merges to main. The image is pulled by the deployment infrastructure (see cf-services repo for docker-compose and Traefik config).

The container runs migrations on startup (`diesel migration run && action-tracker`).

## Diesel Migrations

```bash
# Run migrations
diesel migration run

# Create a new migration
diesel migration generate description_here

# Revert last migration
diesel migration revert
```

After modifying migrations, the schema is auto-generated in `backend/src/db/schema.rs`.
