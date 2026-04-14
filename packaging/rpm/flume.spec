Name:           flume
Version:        1.2.5
Release:        1%{?dist}
Summary:        Modern terminal IRC client
License:        Apache-2.0
URL:            https://github.com/FlumeIRC/flume
Source0:        https://github.com/FlumeIRC/flume/archive/v%{version}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  python3-devel
Requires:       python3

%description
Flume is a fast, modern, terminal-based IRC client built in Rust.
It supports multi-server connections, a rich TUI with theming and
split views, vi/emacs keybinding modes, Lua and Python scripting,
LLM-powered script and theme generation, DCC file transfers,
XDCC, emoji shortcodes, and configurable display formats.

%prep
%autosetup -n flume-%{version}

%build
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
cargo build --release -p flume-tui --features python

%install
install -Dm755 target/release/flume %{buildroot}%{_bindir}/flume
install -Dm644 doc/flume.1 %{buildroot}%{_mandir}/man1/flume.1
install -Dm644 LICENSE %{buildroot}%{_licensedir}/%{name}/LICENSE

%files
%license LICENSE
%{_bindir}/flume
%{_mandir}/man1/flume.1*

%changelog
* Tue Apr 07 2026 Emilio A. Escobar <emilio@flumeirc.io> - 1.2.5-1
- Buffer groups: paired channels as single buffer entries (per-server)
- Mouse support: click buffers, scroll chat, click pane to focus
- /alias command for custom command shortcuts with $* placeholder
- /mouse enable|disable command
- /group create|list|disband commands
- ANSI escape sequence support for colored text and ASCII art
- Miasma theme + transparent (glass) theme variants
- /snotice remove supports ranges (3-7, 1,4,5-8)
- /snotice add strips quotes from --match arguments
- /snotice rules check for duplicates
- /go finds groups by name
- Alt+Shift+1-9 jumps to server by number
- /url open fixed (cached list, no longer pollutes from display)
- /color combo for reusable color combinations (%rainbow%, %alert%)
- CLI args: --version, --help, --nick, --no-autoconnect, --no-autoload-scripts
- Python stable ABI (abi3) — works with any Python 3.7+
- Nix flake support
- flume.version exposed to Lua/Python scripts
- Configurable formats, themes, aliases, mouse, groups all persist via /save

* Thu Apr 03 2026 Emilio A. Escobar <emilio@flumeirc.io> - 1.2.4-1
- Fix Alt+number buffer switching to match displayed order
- Fix active buffer not visually distinct in buffer list
- Alt+0 jumps to buffer 10 (weechat convention)
- /snotice last is now per-server
- FreeBSD x86_64 binary in release builds
- Automated AUR and Homebrew publishing
- CI badges in README

* Wed Apr 02 2026 Emilio A. Escobar <emilio@flumeirc.io> - 1.2.3-1
- User-defined color combinations (%rainbow%, %alert%, cycle combos)
- /color combo add|list|remove|test commands
- Fix snotice rules never matching (duplicate Notice handler)
- Updated /help for all new commands

* Wed Apr 02 2026 Emilio A. Escobar <emilio@flumeirc.io> - 1.2.2-1
- IRC color and formatting support (mIRC colors, 256-color, named colors)
- /color and /colors commands for colored messages
- /snotice suppress for literal text matching
- /snotice last --route with --format support
- ASCII art rendering preserved (selective line wrapping)
- Event pipeline reliability fixes (BATCH protocol, unbounded channels)
- ZNC bouncer compatibility improvements

* Tue Apr 01 2026 Emilio A. Escobar <emilio@flumeirc.io> - 1.2.0-1
- Configurable format strings for all display output
- Regex-based server notice routing for IRC operators
- Buffer numbering matches alphabetical display order

* Tue Apr 01 2026 Emilio A. Escobar <emilio@flumeirc.io> - 1.1.0-1
- Weechat-style TUI layout
- /set command, /go command, emoji shortcodes
- LLM generation, DCC, bouncer support

* Tue Apr 01 2026 Emilio A. Escobar <emilio@flumeirc.io> - 1.0.0-1
- Initial release
