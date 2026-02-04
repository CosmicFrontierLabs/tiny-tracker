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
| GET/PATCH | `/api/vendors/:id` | Get / update vendor |
| GET | `/api/items` | List all items |
| GET/POST | `/api/vendors/:id/items` | List / create items for vendor |
| GET/PATCH | `/api/items/:id` | Get / update item |
| GET/POST | `/api/items/:id/notes` | List / add notes |
| GET | `/api/items/:id/history` | Status history |
| POST | `/api/items/:id/status` | Change status |
| GET | `/api/users` | List users |
| GET | `/api/categories` | List all categories |
| GET/POST | `/api/vendors/:id/categories` | List / create categories for vendor |
| GET | `/go/:item_id` | Deep link redirect |

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
| `DEV_MODE` | No | Set to `true` to bypass OAuth |
| `DEV_USER_ID` | No | User ID for dev mode |

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
