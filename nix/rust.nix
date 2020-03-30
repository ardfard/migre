{ sources ? import ./sources.nix }:

let
pkgs =
import sources.nixpkgs { overlays = [(import sources.nixpkgs-mozilla)]; };
channel = "stable";
date = "2020-03-30";
targets = [];
chan = pkgs.rustChannelOf{ inherit channel; };
in
chan.rust
