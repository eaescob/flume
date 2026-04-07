{
  description = "Flume — modern terminal IRC client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;
    in {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in {
          default = pkgs.rustPlatform.buildRustPackage {
            pname = "flume";
            version = "1.2.5";

            src = self;

            cargoLock = {
              lockFile = ./Cargo.lock;
            };

            cargoBuildFlags = [ "-p" "flume-tui" ];

            env = {
              PYO3_USE_ABI3_FORWARD_COMPATIBILITY = "1";
            };

            postInstall = ''
              mv $out/bin/flume-tui $out/bin/flume 2>/dev/null || true
              install -Dm644 doc/flume.1 $out/share/man/man1/flume.1
            '';

            meta = with pkgs.lib; {
              description = "Modern terminal IRC client with scripting and LLM support";
              homepage = "https://github.com/FlumeIRC/flume";
              license = licenses.asl20;
              mainProgram = "flume";
              platforms = platforms.unix;
            };
          };
        }
      );
    };
}
