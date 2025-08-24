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

  languages.rust.enable = true;
}
