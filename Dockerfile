# Build stage for frontend (WASM)
FROM rust:1.93-bookworm AS frontend-builder

RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY shared ./shared
COPY frontend ./frontend
COPY backend ./backend
COPY cli ./cli

WORKDIR /app/frontend
RUN trunk build --release

# Build stage for backend
FROM rust:1.93-bookworm AS backend-builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY shared ./shared
COPY backend ./backend
COPY cli ./cli
COPY frontend ./frontend
COPY --from=frontend-builder /app/frontend/dist ./frontend/dist

WORKDIR /app/backend
RUN cargo build --release
RUN /app/target/release/action-tracker --check-assets

# Build diesel CLI for migrations
RUN cargo install diesel_cli --no-default-features --features postgres

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /app/target/release/action-tracker /usr/local/bin/action-tracker
COPY --from=backend-builder /usr/local/cargo/bin/diesel /usr/local/bin/diesel
COPY migrations /app/migrations
COPY diesel.toml /app/diesel.toml

WORKDIR /app

EXPOSE 8080

# Run migrations then start the server
CMD diesel migration run && action-tracker
