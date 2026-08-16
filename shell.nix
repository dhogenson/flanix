let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-26.05";
  pkgs = import nixpkgs {config = {}; overlay = [];};
in

pkgs.mkShellNoCC {
  packages = with pkgs; [
    awscli
  ];

  AWS_ENDPOINT_URL="http://localhost:4566";
  AWS_DEFAULT_REGION="us-east-1";
  AWS_ACCESS_KEY_ID="test";
  AWS_SECRET_ACCESS_KEY="test";
}
