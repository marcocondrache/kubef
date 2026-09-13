{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:
{
  packages = [
    pkgs.cargo-make
    pkgs.cargo-edit
    pkgs.cargo-flamegraph
  ];

  languages.rust.enable = true;
}
