#!/usr/bin/env bash
#
# Run parsync S3 integration tests against a local MinIO instance.
#
# Usage:
#   ./scripts/s3_test.sh          # run all S3 e2e tests
#   ./scripts/s3_test.sh basic    # run a single test by substring
#   ./scripts/s3_test.sh --seed   # only start MinIO + seed data, don't run tests
#   ./scripts/s3_test.sh --down   # tear down MinIO
#
# Prerequisites:
#   - Docker & Docker Compose
#   - Rust toolchain (cargo)
#   - aws CLI (for individual e2e tests that use it)

set -euo pipefail

COMPOSE_FILE="docker-compose.s3-test.yml"
PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$PROJECT_ROOT"

# ── Subcommands ────────────────────────────────────────────────────────

if [[ "${1:-}" == "--down" ]]; then
    echo "Tearing down MinIO..."
    docker compose -f "$COMPOSE_FILE" down -v
    exit 0
fi

# ── Start MinIO ────────────────────────────────────────────────────────

echo "Starting MinIO..."
docker compose -f "$COMPOSE_FILE" up -d minio

echo "Waiting for MinIO health check..."
for i in $(seq 1 30); do
    if docker compose -f "$COMPOSE_FILE" exec -T minio curl -sf http://localhost:9000/minio/health/live >/dev/null 2>&1; then
        echo "MinIO is healthy."
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "ERROR: MinIO did not become healthy in 30s" >&2
        docker compose -f "$COMPOSE_FILE" logs minio
        exit 1
    fi
    sleep 1
done

# ── Seed data ──────────────────────────────────────────────────────────

echo "Seeding test data..."
docker compose -f "$COMPOSE_FILE" up seed
echo ""

if [[ "${1:-}" == "--seed" ]]; then
    echo "MinIO is running at http://localhost:9000"
    echo "Console at http://localhost:9001 (testuser / testpass123)"
    echo ""
    echo "Manual testing:"
    echo "  export AWS_ACCESS_KEY_ID=testuser"
    echo "  export AWS_SECRET_ACCESS_KEY=testpass123"
    echo "  export AWS_DEFAULT_REGION=us-east-1"
    echo ""
    echo "  # List objects"
    echo "  aws s3 ls s3://test-bucket/ --endpoint-url http://localhost:9000 --recursive"
    echo ""
    echo "  # Sync from S3"
    echo "  cargo run --features s3 -- -vrPu s3://test-bucket/ /tmp/parsync-test --s3-endpoint-url http://localhost:9000"
    echo ""
    echo "Tear down with: ./scripts/s3_test.sh --down"
    exit 0
fi

# ── Run tests ──────────────────────────────────────────────────────────

echo "Building parsync with S3 feature..."
cargo build --features s3

echo ""
echo "Running S3 e2e tests..."
export AWS_ACCESS_KEY_ID=testuser
export AWS_SECRET_ACCESS_KEY=testpass123
export AWS_DEFAULT_REGION=us-east-1

TEST_FILTER="${1:-}"
if [ -n "$TEST_FILTER" ]; then
    cargo test --features s3 -- --ignored "$TEST_FILTER"
else
    cargo test --features s3 -- --ignored e2e_pull_from_s3
fi

echo ""
echo "All S3 tests passed."
echo "Tear down with: ./scripts/s3_test.sh --down"
