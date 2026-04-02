Name:           flume
Version:        1.2.1
Release:        1%{?dist}
Summary:        Modern terminal IRC client
License:        Apache-2.0
URL:            https://github.com/FlumeIRC/flume
Source0:        https://github.com/FlumeIRC/flume/archive/v%{version}.tar.gz

BuildRequires:  rust >= 1.75
BuildRequires:  cargo
BuildRequires:  python3-devel

%description
Flume is a fast, modern, terminal-based IRC client built in Rust.
It supports multi-server connections, a rich TUI with theming and
split views, vi/emacs keybinding modes, Lua and Python scripting,
LLM-powered script and theme generation, DCC file transfers,
XDCC, emoji shortcodes, and configurable display formats.

%prep
%autosetup -n flume-%{version}

%build
FEATURES=""
python3 -c "import sysconfig" 2>/dev/null && FEATURES="--features python" && export PYO3_USE_ABI3_FORWARD_COMPATIBILITY=1
cargo build --release -p flume-tui $FEATURES

%install
install -Dm755 target/release/flume %{buildroot}%{_bindir}/flume
install -Dm644 doc/flume.1 %{buildroot}%{_mandir}/man1/flume.1
install -Dm644 LICENSE %{buildroot}%{_licensedir}/%{name}/LICENSE

%files
%license LICENSE
%{_bindir}/flume
%{_mandir}/man1/flume.1*

%changelog
* Wed Apr 02 2026 Emilio A. Escobar <emilio@flumeirc.io> - 1.2.1-1
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
