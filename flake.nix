{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [
        "aarch64-darwin"
        "x86_64-darwin"
        "x86_64-linux"
        "aarch64-linux"
      ];
      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [ (import rust-overlay) ];
      };
    in
    {
      devShells = forAllSystems (system:
        let pkgs = pkgsFor system; in {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              (rust-bin.stable.latest.default.override { extensions = ["rust-src" "rust-analyzer"]; })
            ];

            # ort uses load-dynamic: the binary dlopen()s libonnxruntime at runtime.
            # Set ORT_DYLIB_PATH to your local onnxruntime install, e.g.:
            #   macOS (Homebrew): /opt/homebrew/lib/libonnxruntime.dylib
            #   Linux:            /usr/lib/libonnxruntime.so
            # If unset, ort searches standard library paths automatically.
            shellHook = ''
              if [ -z "''${ORT_DYLIB_PATH:-}" ]; then
                # Auto-detect common install locations
                for candidate in \
                    /opt/homebrew/lib/libonnxruntime.dylib \
                    /usr/local/lib/libonnxruntime.dylib \
                    /usr/lib/libonnxruntime.so \
                    /usr/lib/x86_64-linux-gnu/libonnxruntime.so; do
                  if [ -f "$candidate" ]; then
                    export ORT_DYLIB_PATH="$candidate"
                    echo "[med dev] ORT_DYLIB_PATH=$ORT_DYLIB_PATH"
                    break
                  fi
                done
              fi
            '';
          };
        });

      packages = forAllSystems (system:
        let pkgs = pkgsFor system; in {
          med = pkgs.rustPlatform.buildRustPackage {
            pname = "med";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            # ort uses load-dynamic (fastembed's ort-load-dynamic feature):
            # libonnxruntime is dlopen()ed at runtime — no static linking,
            # no framework deps, no onnxruntime at build time.
            meta.mainProgram = "med";
          };
          default = self.packages.${system}.med;
        });
    };
}
