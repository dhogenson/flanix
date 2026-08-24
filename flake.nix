{
  description = "Dev shell";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          awscli
        ];
        AWS_ENDPOINT_URL="http://localhost:4566";
        AWS_DEFAULT_REGION="us-east-1";
        AWS_ACCESS_KEY_ID="test";
        AWS_SECRET_ACCESS_KEY="test";
        DATABASE_URL="postgres://user:password@localhost/mydb";
      };
    };
}
