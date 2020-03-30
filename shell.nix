{ sources ? import ./nix/sources.nix }:
let
nixpkgs = import sources.nixpkgs {};
rustChannel = (import ./nix/rust.nix { inherit sources; }).override {
  extensions = [
    "clippy-preview"
      "rustfmt-preview"
      "rust-src"
      "rust-analysis"
  ];
};
in
with nixpkgs;
pkgs.mkShell {
  buildInputs = [
    rustChannel
      cargo-watch
      kcov
  ];

# Set Environment Variables
  RUST_BACKTRACE = 1;
}

