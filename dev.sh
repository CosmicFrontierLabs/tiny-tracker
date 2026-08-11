#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

case "$1" in
    start)
        echo "Starting development environment..."

        # Start postgres
        docker compose -f docker-compose.dev.yml up -d

        # Wait for postgres to be ready
        echo "Waiting for PostgreSQL to be ready..."
        until docker compose -f docker-compose.dev.yml exec -T postgres pg_isready -U postgres > /dev/null 2>&1; do
            sleep 1
        done
        echo "PostgreSQL is ready!"

        # Create .env if it doesn't exist
        if [ ! -f .env ]; then
            cp .env.example .env
            echo "Created .env from .env.example"
        fi

        # Run migrations
        echo "Running migrations..."
        diesel migration run || echo "Note: Install diesel_cli with 'cargo install diesel_cli --no-default-features --features postgres' to run migrations"

        # Create default dev user and vendor if they don't exist
        echo "Setting up default dev user and vendor..."
        cargo run --quiet --bin action-tracker-cli -- user create --email dev@localhost --name "Dev User" 2>/dev/null || true
        cargo run --quiet --bin action-tracker-cli -- vendor create --prefix DEV --name "Development" 2>/dev/null || true

        echo ""
        echo "Development environment started!"
        echo ""
        echo "To run the backend:"
        echo "  cargo run -p backend"
        echo ""
        echo "To run the frontend (in another terminal):"
        echo "  cd frontend && trunk serve"
        echo ""
        ;;

    stop)
        echo "Stopping development environment..."
        docker compose -f docker-compose.dev.yml down
        echo "Stopped."
        ;;

    status)
        docker compose -f docker-compose.dev.yml ps
        ;;

    logs)
        docker compose -f docker-compose.dev.yml logs -f
        ;;

    reset)
        echo "Resetting database..."
        docker compose -f docker-compose.dev.yml down -v
        echo "Database volume removed. Run './dev.sh start' to recreate."
        ;;

    migrate)
        echo "Running migrations..."
        diesel migration run
        ;;

    *)
        echo "Usage: $0 {start|stop|status|logs|reset|migrate}"
        echo ""
        echo "Commands:"
        echo "  start   - Start PostgreSQL and run migrations"
        echo "  stop    - Stop PostgreSQL"
        echo "  status  - Show container status"
        echo "  logs    - Follow PostgreSQL logs"
        echo "  reset   - Stop and remove database volume"
        echo "  migrate - Run database migrations"
        exit 1
        ;;
esac
