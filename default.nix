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

  cargoHash = "sha256-vc5Szd0UBa5vPZ9+JghIYcVhKgmummZwlVu6Wi6Wg1c=";

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
