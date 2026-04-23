{
  description = "slackslack - a lightweight Slack TUI";

  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0";

  outputs =
    { self, ... }@inputs:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];

      forEachSupportedSystem =
        f:
        inputs.nixpkgs.lib.genAttrs supportedSystems (
          system:
          f {
            inherit system;
            pkgs = import inputs.nixpkgs {
              inherit system;
              config.allowUnfree = true;
            };
          }
        );
    in
    {
      packages = forEachSupportedSystem (
        { pkgs, system }:
        let
          wasmBindgenCliCompat = pkgs.rustPlatform.buildRustPackage {
            pname = "wasm-bindgen-cli";
            version = "0.2.117";
            src = pkgs.fetchurl {
              name = "wasm-bindgen-cli-0.2.117.tar.gz";
              url = "https://crates.io/api/v1/crates/wasm-bindgen-cli/0.2.117/download";
              hash = "sha256-uzYBsomdSIdRK9yq0RUHR1C+fCErEi+n7U+u1skZIp4=";
            };
            cargoHash = "sha256-eKe7uwneUYxejSbG/1hKqg6bSmtL0KQ9ojlazeqTi88=";
            doCheck = false;
          };

          emojiBillboardServer = pkgs.rustPlatform.buildRustPackage {
            pname = "emoji-billboard-server";
            version = "0.1.0";
            src = ./.;
            cargoLock = {
              lockFile = ./Cargo.lock;
            };
            cargoRoot = "crates/emoji-web";
            postPatch = ''
              ln -sf ../../Cargo.lock crates/emoji-web/Cargo.lock
            '';
            nativeBuildInputs = with pkgs; [
              makeWrapper
              python3
              wasmBindgenCliCompat
              wasm-pack
              binaryen
              lld
            ];
            doCheck = false;
            buildPhase = ''
              runHook preBuild
              export HOME="$TMPDIR/home"
              mkdir -p "$HOME"
              export CARGO_TARGET_DIR="$TMPDIR/target"
              cd crates/emoji-web
              wasm-pack build --mode no-install --target web --out-dir "$TMPDIR/pkg"
              runHook postBuild
            '';
            installPhase = ''
              runHook preInstall
              mkdir -p "$out/share/emoji-billboard/static"
              cp -R ${./crates/emoji-web/static}/. "$out/share/emoji-billboard/static"
              chmod -R u+w "$out/share/emoji-billboard/static"
              rm -rf "$out/share/emoji-billboard/static/pkg"
              cp -R "$TMPDIR/pkg" "$out/share/emoji-billboard/static/pkg"
              install -Dm755 ${./crates/emoji-web/serve.py} "$out/share/emoji-billboard/serve.py"

              mkdir -p "$out/bin"
              cat > "$out/bin/emoji-billboard-server" <<EOF
              #!${pkgs.bash}/bin/bash
              set -euo pipefail
              export EMOJI_WEB_STATIC_DIR="$out/share/emoji-billboard/static"
              exec ${pkgs.python3}/bin/python "$out/share/emoji-billboard/serve.py" "\$@"
              EOF
              chmod +x "$out/bin/emoji-billboard-server"
              runHook postInstall
            '';
          };
        in
        {
          emoji-billboard-server = emojiBillboardServer;
          default = emojiBillboardServer;
        }
      );

      apps = forEachSupportedSystem (
        { system, ... }:
        {
          emoji-billboard-server = {
            type = "app";
            program = "${self.packages.${system}.emoji-billboard-server}/bin/emoji-billboard-server";
          };
          default = self.apps.${system}.emoji-billboard-server;
        }
      );

      devShells = forEachSupportedSystem (
        { pkgs, system }:
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              self.formatter.${system}
              rustc
              cargo
              rust-analyzer
              clippy
              rustfmt
              pkg-config
              mold
            ];
          };
        }
      );

      formatter = forEachSupportedSystem ({ pkgs, ... }: pkgs.nixfmt);
    };
}
