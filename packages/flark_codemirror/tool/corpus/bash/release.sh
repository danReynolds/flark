#!/usr/bin/env bash
# Builds a release bundle for the site and publishes it to a staging host.
# Usage: release.sh [--dry-run] [--target NAME] VERSION
set -euo pipefail
shopt -s nullglob extglob

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT=${ROOT:-$SCRIPT_DIR/..}
TARGET="staging"
DRY_RUN=0
declare -a ASSETS=(css js fonts "images/icons")
declare -A CHECKSUMS=()
export LC_ALL=C

log() {
  local level=$1; shift
  printf '%s [%-5s] %s\n' "$(date +%H:%M:%S)" "$level" "$*" >&2
}

die() { log ERROR "$@"; exit 1; }

function usage {
  cat <<EOF
Usage: $(basename "$0") [--dry-run] [--target NAME] VERSION

Options:
  --dry-run       print what would happen
  --target NAME   one of: staging, preview (default: $TARGET)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --target)
      TARGET=${2:?missing target}
      shift
      ;;
    -h|--help) usage; exit 0 ;;
    -*) die "unknown option: $1" ;;
    *) VERSION=$1 ;;
  esac
  shift
done

[ -n "${VERSION:-}" ] || { usage; exit 2; }
if [[ ! $VERSION =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  die "version '$VERSION' is not of the form v1.2.3"
fi

run() {
  if (( DRY_RUN )); then
    echo "would run: $*"
  else
    "$@"
  fi
}

BUILD_DIR=$(mktemp -d "${TMPDIR:-/tmp}/release.XXXXXX")
trap 'rm -rf "$BUILD_DIR"' EXIT INT TERM

log INFO "building $VERSION for $TARGET in $BUILD_DIR"
count=0
for asset in "${ASSETS[@]}"; do
  src="$ROOT/public/$asset"
  if [ ! -d "$src" ]; then
    log WARN "skipping missing $asset"
    continue
  fi
  cp -R "$src" "$BUILD_DIR/" && count=$((count + 1))
done
log INFO "copied $count of ${#ASSETS[@]} asset folders"

# Hash every file; names with spaces survive the NUL-separated pipe.
while IFS= read -r -d '' file; do
  sum=$(shasum -a 256 "$file" | cut -d' ' -f1)
  CHECKSUMS["${file#"$BUILD_DIR"/}"]=$sum
done < <(find "$BUILD_DIR" -type f -print0)

manifest="$BUILD_DIR/manifest.txt"
{
  echo "version=$VERSION"
  echo "built=$(date -u +%Y-%m-%dT%H:%M:%SZ) by ${USER:-unknown}@$(hostname -s)"
  for key in "${!CHECKSUMS[@]}"; do
    printf '%s  %s\n' "${CHECKSUMS[$key]}" "$key"
  done | sort -k2
} > "$manifest"

cat > "$BUILD_DIR/robots.txt" <<'TXT'
User-agent: *
Disallow: /drafts/  # never $expanded here
TXT

sed -e "s/@VERSION@/${VERSION#v}/g" \
    -e 's/@CHANNEL@/'"$TARGET"'/' \
  "$ROOT/templates/index.html" > "$BUILD_DIR/index.html"

tarball="site-${VERSION}.tar.gz"
tar -C "$BUILD_DIR" -czf "$tarball" . 2>/dev/null || die "tar failed ($?)"
size=$(wc -c < "$tarball" | tr -d ' ')
log INFO "bundle $tarball is $((size / 1024)) KiB"

host=$(git config --get "release.$TARGET.host" || echo "$TARGET.example.test")
until run ssh -o BatchMode=yes "deploy@$host" true; do
  (( ++attempts > 3 )) && die "cannot reach $host"
  sleep $(( attempts * 2 ))
done
run scp "$tarball" "deploy@$host:/srv/site/releases/"
grep -q '^version=' "$manifest" && echo 'manifest ok' | tee -a release.log
echo `git rev-parse --short HEAD` > REVISION
echo "Price: \$5, path: ~/sites, glob: *.html, done: 100%"
exit 0
