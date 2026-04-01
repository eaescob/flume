# Publishing Flume to Package Managers

This document covers how to publish Flume releases to each supported packaging system.

## Pre-release Checklist

1. Update version in `Cargo.toml` (workspace-level `version` field)
2. Update `doc/flume.1` date and version in the `.TH` line
3. Run `cargo test` and `cargo clippy` — everything must pass
4. Create a git tag: `git tag v1.1.0 && git push origin v1.1.0`
5. Create a GitHub Release from the tag, attach pre-built binaries

## Building Release Binaries

```sh
# macOS (Apple Silicon)
cargo build --release -p flume-tui
# Binary at: target/release/flume-tui

# macOS (Intel) — cross-compile
cargo build --release -p flume-tui --target x86_64-apple-darwin

# Linux (x86_64) — use cross or a container
cross build --release -p flume-tui --target x86_64-unknown-linux-gnu

# FreeBSD — build on a FreeBSD machine or VM
cargo build --release -p flume-tui
```

With Python scripting:
```sh
cargo build --release -p flume-tui --features python
```

## Homebrew

**File:** `packaging/homebrew/flume.rb`

### Setup (first time)

1. Create a Homebrew tap repo: `https://github.com/emilio/homebrew-tap`
2. Copy `packaging/homebrew/flume.rb` into the tap repo

### Publishing a new version

1. Build the release and create a tarball of the source:
   ```sh
   git archive --format=tar.gz --prefix=flume-1.1.0/ v1.1.0 > flume-1.1.0.tar.gz
   ```

2. Calculate the SHA256:
   ```sh
   shasum -a 256 flume-1.1.0.tar.gz
   ```

3. Update `packaging/homebrew/flume.rb`:
   - Change the `url` to point to the new release tarball on GitHub
   - Replace `PLACEHOLDER_SHA256` with the actual SHA256
   - Update `distversion` if changed

4. Copy the updated formula to the tap repo and push:
   ```sh
   cp packaging/homebrew/flume.rb ../homebrew-tap/Formula/flume.rb
   cd ../homebrew-tap && git add -A && git commit -m "flume 1.1.0" && git push
   ```

5. Test:
   ```sh
   brew install emilio/tap/flume
   ```

## Arch Linux (AUR)

**File:** `packaging/aur/PKGBUILD`

### Setup (first time)

1. Create an AUR account at https://aur.archlinux.org
2. Clone the AUR package: `git clone ssh://aur@aur.archlinux.org/flume.git`
3. Copy `packaging/aur/PKGBUILD` and `.SRCINFO` into it

### Publishing a new version

1. Update `packaging/aur/PKGBUILD`:
   - Set `pkgver` to the new version
   - Download the source tarball and compute sha256sum
   - Replace `PLACEHOLDER` with the actual SHA256

2. Generate `.SRCINFO`:
   ```sh
   cd packaging/aur
   makepkg --printsrcinfo > .SRCINFO
   ```

3. Test the build:
   ```sh
   makepkg -si
   ```

4. Push to AUR:
   ```sh
   cd /path/to/aur/flume
   cp /path/to/flume/packaging/aur/PKGBUILD .
   cp /path/to/flume/packaging/aur/.SRCINFO .
   git add -A && git commit -m "Update to 1.1.0" && git push
   ```

## FreeBSD Ports

**File:** `packaging/freebsd/Makefile`

### Setup (first time)

FreeBSD ports are submitted via Bugzilla: https://bugs.freebsd.org

1. Prepare the port directory with `Makefile`, `pkg-descr`, `pkg-plist`
2. Test with `make stage && make check-plist`
3. Submit a PR via `porttools` or Bugzilla

### Publishing a new version

1. Update `packaging/freebsd/Makefile`:
   - Set `DISTVERSION` to the new version

2. Regenerate the distinfo:
   ```sh
   make makesum
   ```

3. Test:
   ```sh
   make stage && make check-plist
   ```

4. Submit an update PR to the FreeBSD ports tree, or if you're a committer, commit directly.

## crates.io (Rust)

To publish flume-core and flume-tui as Rust crates:

```sh
# Publish core first (it's a dependency)
cd flume-core && cargo publish

# Then the TUI
cd ../flume-tui && cargo publish
```

Users can then install with:
```sh
cargo install flume-tui
```

### Pre-publish checks

- Ensure `Cargo.toml` has `description`, `license`, `repository`, `readme` fields
- Run `cargo package --list` to verify what gets included
- Check `cargo package` builds cleanly

## GitHub Actions (CI/CD)

For automated releases, create `.github/workflows/release.yml`:

```yaml
name: Release
on:
  push:
    tags: ['v*']

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: macos-latest
            target: aarch64-apple-darwin
          - os: macos-latest
            target: x86_64-apple-darwin
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build --release -p flume-tui --target ${{ matrix.target }}
      - uses: softprops/action-gh-release@v2
        with:
          files: target/${{ matrix.target }}/release/flume-tui
```

This builds binaries for each platform and attaches them to the GitHub Release automatically.
