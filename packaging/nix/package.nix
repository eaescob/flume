{
  lib,
  rustPlatform,
  fetchFromGitHub,
  python3,
  pkg-config,
  openssl,
  withPython ? true,
}:

rustPlatform.buildRustPackage (finalAttrs: {
  pname = "flume";
  version = "1.2.4";

  src = fetchFromGitHub {
    owner = "FlumeIRC";
    repo = "flume";
    tag = "v${finalAttrs.version}";
    hash = lib.fakeHash;
  };

  cargoHash = lib.fakeHash;

  cargoBuildFlags = [ "-p" "flume-tui" ];

  buildInputs = lib.optionals withPython [
    python3
  ];

  nativeBuildInputs = [
    pkg-config
  ];

  buildFeatures = lib.optionals withPython [ "python" ];

  env = lib.optionalAttrs withPython {
    PYO3_USE_ABI3_FORWARD_COMPATIBILITY = "1";
  };

  # Rename the binary from flume-tui to flume
  postInstall = ''
    mv $out/bin/flume-tui $out/bin/flume 2>/dev/null || true
    install -Dm644 doc/flume.1 $out/share/man/man1/flume.1
  '';

  meta = {
    description = "Modern terminal IRC client with scripting and LLM support";
    homepage = "https://github.com/FlumeIRC/flume";
    license = lib.licenses.asl20;
    maintainers = [ ];
    mainProgram = "flume";
    platforms = lib.platforms.unix;
  };
})
