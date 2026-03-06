{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "aarch64-darwin"; # Use "x86_64-darwin" for Intel or "x86_64-linux"
      pkgs = import nixpkgs { inherit system; overlays = [ (import rust-overlay) ]; };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          # Just the compiler and the analyzer
          (rust-bin.stable.latest.default.override { extensions = ["rust-src" "rust-analyzer"]; })
        ];
      };
    };
}
