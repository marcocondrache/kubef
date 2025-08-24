{
  rustPlatform,
  installShellFiles,
  stdenv,
  libiconv,
  lib,
}:
let
  manifest = lib.importTOML ./Cargo.toml;
in
rustPlatform.buildRustPackage rec {
  pname = manifest.package.name;
  version = manifest.package.version;

  src = lib.cleanSource ./.;

  nativeBuildInputs = [ installShellFiles ];

  buildInputs = lib.optionals stdenv.hostPlatform.isDarwin [ libiconv ];

  cargoHash = "sha256-gapsrZwqEb/ob/sj5l+AwxE5lw+wSddy5ZKedrjXG64=";

  meta = with lib; {
    description = "A tool to help managing kubernetes forwarders";
    mainProgram = manifest.package.name;
    longDescription = ''
      Kubef is a tool to help managing kubernetes forwarders.
    '';
    homepage = "https://github.com/marcocondrache/kubef";
    license = licenses.mit;
  };
}
