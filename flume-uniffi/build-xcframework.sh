#!/usr/bin/env bash
# Build the FlumeCore.xcframework + Swift glue consumed by FlumeMac.
#
# Outputs:
#   target/FlumeCore.xcframework
#   target/Generated/FlumeCore.swift
#
# arm64-only — add x86_64 / iOS slices when those targets are needed.

set -euo pipefail

CRATE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_DIR="$(cd "$CRATE_DIR/.." && pwd)"
cd "$WORKSPACE_DIR"

TARGET="aarch64-apple-darwin"
PROFILE="release"
LIB_NAME="flume_uniffi"
FRAMEWORK_NAME="FlumeCore"

TARGET_DIR="$WORKSPACE_DIR/target"
GEN_DIR="$TARGET_DIR/Generated"
STATIC_LIB="$TARGET_DIR/$TARGET/$PROFILE/lib$LIB_NAME.a"
DYLIB="$TARGET_DIR/$TARGET/$PROFILE/lib$LIB_NAME.dylib"
XCFRAMEWORK="$TARGET_DIR/$FRAMEWORK_NAME.xcframework"

echo "==> cargo build (target=$TARGET, profile=$PROFILE)"
# Pin macOS deployment target to match the SwiftPM consumer (macOS 14)
# so the resulting .a doesn't trip ld warnings about newer SDK objects.
MACOSX_DEPLOYMENT_TARGET=14.0 \
    cargo build --release -p flume-uniffi --target "$TARGET"

echo "==> uniffi-bindgen → Swift"
mkdir -p "$GEN_DIR"
cargo run --release -p flume-uniffi --bin uniffi-bindgen -- \
    generate --library "$DYLIB" --language swift --out-dir "$GEN_DIR"

SWIFT_FILE="$GEN_DIR/$LIB_NAME.swift"
HEADER_FILE="$GEN_DIR/${LIB_NAME}FFI.h"
MODULEMAP_FILE="$GEN_DIR/${LIB_NAME}FFI.modulemap"

if [ ! -f "$SWIFT_FILE" ] || [ ! -f "$HEADER_FILE" ] || [ ! -f "$MODULEMAP_FILE" ]; then
    echo "error: expected uniffi-bindgen outputs in $GEN_DIR" >&2
    ls -la "$GEN_DIR" >&2
    exit 1
fi

mv "$MODULEMAP_FILE" "$GEN_DIR/module.modulemap"
mv "$SWIFT_FILE" "$GEN_DIR/$FRAMEWORK_NAME.swift"

echo "==> assembling $FRAMEWORK_NAME.xcframework"
rm -rf "$XCFRAMEWORK"

# Hand-roll the xcframework layout so this script doesn't require a full
# Xcode install (Command Line Tools alone don't ship xcodebuild).
SLICE_DIR="$XCFRAMEWORK/macos-arm64"
HEADERS_DIR="$SLICE_DIR/Headers"
mkdir -p "$HEADERS_DIR"
cp "$STATIC_LIB" "$SLICE_DIR/lib$LIB_NAME.a"
cp "$HEADER_FILE" "$HEADERS_DIR/"
cp "$GEN_DIR/module.modulemap" "$HEADERS_DIR/"

cat > "$XCFRAMEWORK/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>AvailableLibraries</key>
  <array>
    <dict>
      <key>LibraryIdentifier</key>
      <string>macos-arm64</string>
      <key>LibraryPath</key>
      <string>lib$LIB_NAME.a</string>
      <key>HeadersPath</key>
      <string>Headers</string>
      <key>SupportedArchitectures</key>
      <array><string>arm64</string></array>
      <key>SupportedPlatform</key>
      <string>macos</string>
    </dict>
  </array>
  <key>CFBundlePackageType</key>
  <string>XFWK</string>
  <key>XCFrameworkFormatVersion</key>
  <string>1.0</string>
</dict>
</plist>
EOF

echo
echo "OK"
echo "  xcframework: $XCFRAMEWORK"
echo "  swift glue:  $GEN_DIR/$FRAMEWORK_NAME.swift"
