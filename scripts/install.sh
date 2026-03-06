#!/usr/bin/env bash
set -euo pipefail

REPO="${PARSYNC_REPO:-AlpinDale/parsync}"
BIN_NAME="parsync"
INSTALL_DIR="${PARSYNC_INSTALL_DIR:-}"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

download() {
  local url="$1"
  local out="$2"
  if have_cmd curl; then
    curl -fsSL "$url" -o "$out"
  elif have_cmd wget; then
    wget -qO "$out" "$url"
  else
    echo "error: need curl or wget" >&2
    exit 1
  fi
}

read_latest_tag() {
  local json_file="$1"
  if have_cmd jq; then
    jq -r '.tag_name' < "$json_file"
  else
    sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' "$json_file" | head -n1
  fi
}

detect_platform() {
  local os arch
  os="$(uname -s | tr '[:upper:]' '[:lower:]')"
  arch="$(uname -m | tr '[:upper:]' '[:lower:]')"

  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *)
      echo "error: unsupported architecture: $arch" >&2
      exit 1
      ;;
  esac

  case "$os" in
    linux) echo "${arch}-linux" ;;
    darwin) echo "${arch}-macos" ;;
    *)
      echo "error: unsupported OS: $os (this script supports Linux/macOS)" >&2
      exit 1
      ;;
  esac
}

choose_install_dir() {
  if [[ -n "$INSTALL_DIR" ]]; then
    echo "$INSTALL_DIR"
    return
  fi

  if [[ -w /usr/local/bin ]]; then
    echo "/usr/local/bin"
    return
  fi

  echo "${HOME}/.local/bin"
}

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

meta_json="${tmpdir}/latest.json"
download "$API_URL" "$meta_json"

tag="$(read_latest_tag "$meta_json")"
if [[ -z "${tag}" || "${tag}" == "null" ]]; then
  echo "error: failed to read latest release tag from ${API_URL}" >&2
  exit 1
fi

platform="$(detect_platform)"
asset="${BIN_NAME}-${tag}-${platform}.tar.gz"
asset_url="https://github.com/${REPO}/releases/download/${tag}/${asset}"
archive="${tmpdir}/${asset}"

echo "[parsync] downloading ${asset_url}"
download "$asset_url" "$archive"

tar -xzf "$archive" -C "$tmpdir"
if [[ ! -f "${tmpdir}/${BIN_NAME}" ]]; then
  echo "error: archive did not contain ${BIN_NAME}" >&2
  exit 1
fi

dest_dir="$(choose_install_dir)"
mkdir -p "$dest_dir"
install -m 755 "${tmpdir}/${BIN_NAME}" "${dest_dir}/${BIN_NAME}"

echo "[parsync] installed to ${dest_dir}/${BIN_NAME}"
if [[ ":${PATH}:" != *":${dest_dir}:"* ]]; then
  echo "[parsync] ${dest_dir} is not in PATH."
  echo "[parsync] add this line to your shell profile:"
  echo "export PATH=\"${dest_dir}:\$PATH\""
fi

"${dest_dir}/${BIN_NAME}" --version || true
