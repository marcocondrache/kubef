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
  };

  packages = [
    pkgs.cargo-make
  ];

  languages.rust.enable = true;
}
