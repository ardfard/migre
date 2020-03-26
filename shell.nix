{ sources ? import ./nix/sources.nix }:
let
moz_overlay = import sources.nixpkgs-mozilla;
nixpkgs = import sources.nixpkgs { overlays = [moz_overlay]; };
rustChannel = nixpkgs.latest.rustChannels.stable.rust.override {
  extensions = [
    "rust-src"
      "rls-preview"
      "clippy-preview"
      "rustfmt-preview"
  ];
};
in
with nixpkgs;
stdenv.mkDerivation {
  name = "rust-env";
  buildInputs = [
    rustChannel
      rls
      rustup
      cargo-watch
      kcov
      ansible
      sshpass
  ];

# Set Environment Variables
  RUST_BACKTRACE = 1;
}

