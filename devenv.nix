{
  pkgs,
  lib,
  config,
  inputs,
  ...
}:
{
  env = {
    KUBECONFIG = "${config.env.DEVENV_ROOT}/../hive/kubeconfig";
    KUBEF_CONFIG_PATH = "${config.env.DEVENV_ROOT}/kubef.yaml";
  };

  languages.rust.enable = true;
}
