#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
IMAGE_NAME="parsync-bench"
RESULTS_DIR="$PROJECT_DIR/bench-results"

mkdir -p "$RESULTS_DIR"

echo "==> Building benchmark Docker image..."
docker build -f "$PROJECT_DIR/Dockerfile.bench" -t "$IMAGE_NAME" "$PROJECT_DIR"

stamp=$(date +%Y%m%d-%H%M%S)
json_file="$RESULTS_DIR/bench-$stamp.json"
md_file="$RESULTS_DIR/bench-$stamp.md"

echo "==> Running benchmarks..."
# JSON goes to stdout, progress + markdown table go to stderr
docker run --rm "$IMAGE_NAME" > "$json_file" 2> "$md_file"

echo "==> Results:"
cat "$md_file"
echo
echo "JSON: $json_file"
echo "Markdown: $md_file"
