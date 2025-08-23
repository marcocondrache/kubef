{
  rustPlatform,
  fetchFromGitHub,
  installShellFiles,
  stdenv,
  libiconv,
  lib,
}:

rustPlatform.buildRustPackage rec {
  pname = "kubef";
  version = "0.1.0";

  src = fetchFromGitHub {
    owner = "marcocondrache";
    repo = pname;
    tag = "v${version}";
    sha256 = "sha256-8OEyOZbJCzgFJit+A7Q/m8ewGhIz0PZA1fWTH41a44E=";
  };

  nativeBuildInputs = [ installShellFiles ];

  buildInputs = lib.optionals stdenv.hostPlatform.isDarwin [ libiconv ];

  cargoHash = "sha256-WjK0nBfP26b8JDRhBWyE0nsXBajez0MpU6N5l5fZZkM=";

  meta = with lib; {
    description = "A tool to help managing kubernetes forwarders";
    mainProgram = "kubef";
    longDescription = ''
      Kubef is a tool to help managing kubernetes forwarders.
    '';
    homepage = "https://github.com/marcocondrache/kubef";
    license = licenses.mit;
  };
}
