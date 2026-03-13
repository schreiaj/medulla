#!/usr/bin/env bash
# Usage: scripts/update-flake-hashes.sh <version>
# Updates medVersion and all four med binary sha256 hashes in flake.nix.
# Requires: nix (for nix-prefetch-url and nix hash convert), python3.
set -euo pipefail

VERSION="${1:?Usage: $0 <version>  (without leading v)}"
FLAKE="$(git rev-parse --show-toplevel)/flake.nix"
BASE_URL="https://github.com/schreiaj/medulla/releases/download/v${VERSION}"

fetch_sri() {
    local url="$1"
    local b32
    b32=$(nix-prefetch-url --unpack "$url" 2>/dev/null)
    nix hash convert --hash-algo sha256 --to sri "$b32"
}

echo "Fetching hashes for v${VERSION} (all platforms in parallel)..."

fetch_sri "${BASE_URL}/med-aarch64-apple-darwin.tar.gz"   > /tmp/hash_darwin_arm &
fetch_sri "${BASE_URL}/med-x86_64-apple-darwin.tar.gz"    > /tmp/hash_darwin_x86 &
fetch_sri "${BASE_URL}/med-x86_64-unknown-linux-gnu.tar.gz" > /tmp/hash_linux_x86 &
fetch_sri "${BASE_URL}/med-aarch64-unknown-linux-gnu.tar.gz" > /tmp/hash_linux_arm &
wait

DARWIN_ARM=$(cat /tmp/hash_darwin_arm)
DARWIN_X86=$(cat /tmp/hash_darwin_x86)
LINUX_X86=$(cat /tmp/hash_linux_x86)
LINUX_ARM=$(cat /tmp/hash_linux_arm)

echo "  aarch64-darwin: $DARWIN_ARM"
echo "  x86_64-darwin:  $DARWIN_X86"
echo "  x86_64-linux:   $LINUX_X86"
echo "  aarch64-linux:  $LINUX_ARM"

python3 - "$FLAKE" "$VERSION" "$DARWIN_ARM" "$DARWIN_X86" "$LINUX_X86" "$LINUX_ARM" <<'EOF'
import sys, re

flake, version, darwin_arm, darwin_x86, linux_x86, linux_arm = sys.argv[1:]

with open(flake) as f:
    content = f.read()

content = re.sub(r'medVersion = "[^"]*"', f'medVersion = "{version}"', content)

hashes = {
    "med-aarch64-apple-darwin.tar.gz":    darwin_arm,
    "med-x86_64-apple-darwin.tar.gz":     darwin_x86,
    "med-x86_64-unknown-linux-gnu.tar.gz": linux_x86,
    "med-aarch64-unknown-linux-gnu.tar.gz": linux_arm,
}

for archive, sha in hashes.items():
    content = re.sub(
        r'(archive = "' + re.escape(archive) + r'";\s*\n\s*sha256\s*=\s*)"[^"]*"',
        r'\g<1>"' + sha + '"',
        content,
    )

with open(flake, "w") as f:
    f.write(content)

print("flake.nix updated.")
EOF
