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
  version = "3.0.32";

  src = fetchFromGitHub {
    owner = "marcocondrache";
    repo = pname;
    rev = "v${version}";
    sha256 = "sha256-pLVR/vD7wMH/8UziWe5nwL/fBrexg1BtiJouRb73L4E=";
  };

  nativeBuildInputs = [ installShellFiles ];

  buildInputs = lib.optionals stdenv.hostPlatform.isDarwin [ libiconv ];

  cargoHash = "sha256-Nonid/5Jh0WIQV0G3fpmkW0bql6bvlcNJBMZ+6MTTPQ=";

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
