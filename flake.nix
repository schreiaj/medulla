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
              onnxruntime
            ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
              Foundation
              Security
              CoreFoundation
            ]);

            # Point ort-sys at the system onnxruntime so it doesn't try to download
            # the binary in dev shells either.
            ORT_LIB_LOCATION = "${pkgs.onnxruntime}/lib";
            ORT_PREFER_DYNAMIC_LINK = "1";
          };
        });

      packages = forAllSystems (system:
        let pkgs = pkgsFor system; in {
          med = pkgs.rustPlatform.buildRustPackage {
            pname = "med";
            version = "0.1.0";
            src = ./.;
            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = with pkgs; [ pkg-config ];

            buildInputs = with pkgs; [ onnxruntime ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs.darwin.apple_sdk.frameworks; [
                Foundation
                Security
                CoreFoundation
              ]);

            # Use the nixpkgs onnxruntime instead of downloading a binary.
            # ORT_LIB_LOCATION is checked by ort-sys before attempting any download,
            # so this works even with the ort-download-binaries Cargo feature enabled.
            ORT_LIB_LOCATION = "${pkgs.onnxruntime}/lib";
            ORT_PREFER_DYNAMIC_LINK = "1";

            meta.mainProgram = "med";
          };
          default = self.packages.${system}.med;
        });
    };
}
