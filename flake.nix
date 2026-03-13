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

      # Fetch the prebuilt ORT 1.20.0 dylib directly from the official GitHub
      # release rather than relying on nixpkgs — nixpkgs's onnxruntime is broken
      # on Darwin (removed apple_sdk_11_0 dep) and ships the wrong version on
      # older channels (1.18.1 vs the required 1.20.x).
      #
      # To update hashes after bumping ortVersion, delete the old hash strings
      # and run `nix develop` — Nix will fail and print the correct hash.
      ortVersion = "1.20.0";
      ortPlatformInfo = {
        "aarch64-darwin" = {
          url    = "https://github.com/microsoft/onnxruntime/releases/download/v${ortVersion}/onnxruntime-osx-arm64-${ortVersion}.tgz";
          sha256 = "sha256-K8+q+p/wo6lPeOOvLxNf/eW7LXmwjoOlDbxFCw0g3a4=";
          dylib  = "libonnxruntime.${ortVersion}.dylib";
        };
        "x86_64-darwin" = {
          url    = "https://github.com/microsoft/onnxruntime/releases/download/v${ortVersion}/onnxruntime-osx-x86_64-${ortVersion}.tgz";
          sha256 = "sha256-JC0c1zZuMycS3EgLUvKj/uYg1wMJ9W+5AJjwlG29m5Y=";
          dylib  = "libonnxruntime.${ortVersion}.dylib";
        };
        "x86_64-linux" = {
          url    = "https://github.com/microsoft/onnxruntime/releases/download/v${ortVersion}/onnxruntime-linux-x64-${ortVersion}.tgz";
          sha256 = "sha256-Qfy8uaLrtwR1ESz6ZJ+uw1vJKqhK/ZiHcn5qVvgtTnc=";
          dylib  = "libonnxruntime.so.${ortVersion}";
        };
        "aarch64-linux" = {
          url    = "https://github.com/microsoft/onnxruntime/releases/download/v${ortVersion}/onnxruntime-linux-aarch64-${ortVersion}.tgz";
          sha256 = "sha256-V5o0X71Z85oCSINGrZxNomdomK9dKIhtAXYcDGlh/Gw=";
          dylib  = "libonnxruntime.so.${ortVersion}";
        };
      };

      ortFor = system:
        let
          pkgs = pkgsFor system;
          info = ortPlatformInfo.${system};
        in pkgs.stdenv.mkDerivation {
          name = "onnxruntime-prebuilt-${ortVersion}";
          src = pkgs.fetchurl {
            url    = info.url;
            sha256 = info.sha256;
          };
          # The tarball unpacks to onnxruntime-{platform}-{version}/lib/…
          sourceRoot = ".";
          installPhase = ''
            mkdir -p $out/lib
            # Exclude *.dSYM bundles — they contain a file with the same name but
            # mach-o type 10 (MH_DSYM) which dlopen cannot load.
            find . -name "${info.dylib}" -not -path "*.dSYM/*" -exec cp {} $out/lib/ \;
            # Provide a bare soname so ORT_DYLIB_PATH can point to a stable path
            ln -sf $out/lib/${info.dylib} $out/lib/libonnxruntime${if pkgs.stdenv.isDarwin then ".dylib" else ".so"}
          '';
          dontBuild = true;
          dontFixup = true;
        };

    in
    {
      devShells = forAllSystems (system:
        let
          pkgs  = pkgsFor system;
          ort   = ortFor system;
          dylib = if pkgs.stdenv.isDarwin
                  then "${ort}/lib/libonnxruntime.dylib"
                  else "${ort}/lib/libonnxruntime.so";
        in {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              (rust-bin.stable.latest.default.override { extensions = ["rust-src" "rust-analyzer"]; })
            ];

            # ort uses load-dynamic: point it at the prebuilt dylib fetched above.
            ORT_DYLIB_PATH = dylib;
          };
        });

      packages = forAllSystems (system:
        let pkgs = pkgsFor system; in {
          med = pkgs.rustPlatform.buildRustPackage {
            pname = "med";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            # ort uses load-dynamic: no static linking, no ORT at build time.
            meta.mainProgram = "med";
          };
          default = self.packages.${system}.med;
        });
    };
}
