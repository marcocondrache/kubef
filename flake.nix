{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
  };

  outputs =
    inputs:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "aarch64-linux"
        "x86_64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];

      imports = [
        inputs.flake-parts.flakeModules.easyOverlay
      ];

      perSystem =
        {
          config,
          system,
          pkgs,
          ...
        }:
        {
          overlayAttrs = {
            inherit (config.packages) kubef;
          };

          packages = rec {
            default = kubef;
            kubef = pkgs.callPackage ./default.nix { };
          };
        };
    };
}
