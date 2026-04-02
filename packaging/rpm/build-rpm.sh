#!/bin/bash
# Build an RPM package for Flume
# Usage: ./build-rpm.sh
#
# Requires: cargo, rpmbuild, rpmdevtools
# Run from the repo root directory

set -e

VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')

echo "Building flume v${VERSION} RPM..."

# Set up rpmbuild tree
rpmdev-setuptree 2>/dev/null || true

# Create source tarball
git archive --format=tar.gz --prefix="flume-${VERSION}/" HEAD > "$HOME/rpmbuild/SOURCES/v${VERSION}.tar.gz"

# Copy spec file
cp packaging/rpm/flume.spec "$HOME/rpmbuild/SPECS/"

# Build RPM
rpmbuild -ba "$HOME/rpmbuild/SPECS/flume.spec"

echo "RPM built in ~/rpmbuild/RPMS/"
