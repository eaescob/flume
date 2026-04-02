#!/bin/bash
# Build a .deb package for Flume
# Usage: ./build-deb.sh [version]
#
# Requires: cargo, dpkg-deb
# Run from the repo root directory

set -e

VERSION="${1:-$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')}"
ARCH="amd64"
PKG="flume_${VERSION}_${ARCH}"

echo "Building flume v${VERSION} .deb package..."

# Detect Python and enable feature if available
FEATURES=""
if python3 -c "import sysconfig; print(sysconfig.get_path('include'))" 2>/dev/null; then
  echo "Python detected, enabling python feature"
  FEATURES="--features python"
  export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
fi

# Build the binary
cargo build --release -p flume-tui $FEATURES

# Create package structure
rm -rf "target/${PKG}"
mkdir -p "target/${PKG}/DEBIAN"
mkdir -p "target/${PKG}/usr/bin"
mkdir -p "target/${PKG}/usr/share/man/man1"
mkdir -p "target/${PKG}/usr/share/doc/flume"

# Copy files
cp target/release/flume "target/${PKG}/usr/bin/"
cp doc/flume.1 "target/${PKG}/usr/share/man/man1/"
gzip -9 "target/${PKG}/usr/share/man/man1/flume.1"
cp LICENSE "target/${PKG}/usr/share/doc/flume/"

# Write control file with correct version
sed "s/^Version:.*/Version: ${VERSION}/" packaging/deb/control > "target/${PKG}/DEBIAN/control"

# Set permissions
chmod 755 "target/${PKG}/usr/bin/flume"

# Build .deb
dpkg-deb --build "target/${PKG}"

echo "Built: target/${PKG}.deb"
