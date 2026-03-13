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
        let
          pkgs = pkgsFor system;

          # Prebuilt release binaries — statically linked ORT, no compile step.
          # Update medVersion and hashes after each release:
          #   delete the sha256 values, run `nix build`, let Nix print the correct hashes.
          medVersion = "0.1.0";
          medBinaryInfo = {
            "aarch64-darwin" = {
              archive = "med-aarch64-apple-darwin.tar.gz";
              sha256  = "sha256-td+Ltes1r22X9Q6sM5Uy9ZbASRo9G52gbhyI3KzKYxk=";
            };
            "x86_64-darwin" = {
              archive = "med-x86_64-apple-darwin.tar.gz";
              sha256  = "sha256-YxD+RXlTo4SSYC6uMdAoonO9SCl5nX9cy6T33qH4HpY=";
            };
            "x86_64-linux" = {
              archive = "med-x86_64-unknown-linux-gnu.tar.gz";
              sha256  = "sha256-COwaAiFGmxUnHe2H9HL9t6Y9KIB9NF1mdTmUajQp3mo=";
            };
            "aarch64-linux" = {
              archive = "med-aarch64-unknown-linux-gnu.tar.gz";
              sha256  = "sha256-4FGkhY8IU6Jdr90/F+mqeEoIIwzkXfeltf7C0Cfy/rI=";
            };
          };
          info = medBinaryInfo.${system};
        in {
          med = pkgs.stdenv.mkDerivation {
            pname = "med";
            version = medVersion;
            src = pkgs.fetchurl {
              url    = "https://github.com/schreiaj/medulla/releases/download/v${medVersion}/${info.archive}";
              sha256 = info.sha256;
            };
            # On Linux/NixOS the binary references glibc at a non-Nix path;
            # autoPatchelfHook rewrites the dynamic linker and rpath automatically.
            nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.autoPatchelfHook
            ];
            buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux [
              pkgs.stdenv.cc.libc
            ];
            # The tarball contains a bare binary with no wrapping directory.
            sourceRoot = ".";
            installPhase = ''
              mkdir -p $out/bin
              cp med $out/bin/
            '';
            dontBuild  = true;
            # Let autoPatchelfHook run on Linux; skip fixup on Darwin.
            dontFixup  = pkgs.stdenv.isDarwin;
            meta.mainProgram = "med";
          };
          default = self.packages.${system}.med;

          # Build from source (useful for development / CI):
          #   nix build .#med-src
          med-src = pkgs.rustPlatform.buildRustPackage {
            pname = "med";
            version = medVersion;
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;
            meta.mainProgram = "med";
          };
        });
    };
}
