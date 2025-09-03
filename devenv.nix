{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:
{
  env = {
    KUBECONFIG = "${config.env.DEVENV_ROOT}/../../Work/kubeconfig";
    KUBEF_CONFIG = "${config.env.DEVENV_ROOT}/../../Work/kubef.yaml";
    KUBEF_LOG = "debug";
  };

  packages = [
    pkgs.cargo-make
    pkgs.cargo-edit
    pkgs.cargo-flamegraph
  ];

  languages.rust.enable = true;
}
