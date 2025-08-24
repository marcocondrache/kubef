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
    KUBEF_PATH_CONFIG = "${config.env.DEVENV_ROOT}/kubef.json";
  };

  languages.rust.enable = true;
}
