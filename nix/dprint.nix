# dprint with its plugins from nix, so formatting downloads nothing and
# flake.lock pins the plugin versions. dprint.jsonc holds only their settings.
#
# dprint takes `--plugins` only after a subcommand that formats, and the flag
# swallows every argument after it, so it goes last.
{
  pkgs,
  workspace,
}: let
  inherit (pkgs) lib fetchurl;

  # Plugins nixpkgs does not package. The hashes are from
  # https://plugins.dprint.dev/info.json.
  alejandra = fetchurl {
    url = "https://plugins.dprint.dev/alejandra-0.2.0.wasm";
    sha256 = "1sc832q32a6a6k03gm1g2vw57hf6wzprprdvf3gx7lk9nc5scng2";
  };

  shfmt = fetchurl {
    url = "https://plugins.dprint.dev/shfmt-0.2.0.wasm";
    sha256 = "02dmi4v0dcxk8sgm98syy2aqr2g29m73pysb0md77qhyrkm33ykr";
  };

  wasmPlugins =
    map (plugin: "${plugin}/plugin.wasm") (
      with pkgs.dprint-plugins; [
        dprint-plugin-biome
        dprint-plugin-markdown
        dprint-plugin-toml
        g-plane-malva # SCSS
        g-plane-markup_fmt # HTML
      ]
    )
    ++ [
      alejandra
      shfmt
    ];

  # The exec plugin runs the formatters below. It is a native binary, which
  # dprint loads from a manifest that names a zip and the zip's checksum. The
  # musl builds are static, so they run on NixOS. Checksums are from
  # https://plugins.dprint.dev/exec-0.9.0.json.
  execBuilds = {
    x86_64-linux = {
      target = "x86_64-unknown-linux-musl";
      platform = "linux-x86_64";
      sha256 = "d3fcae08e38f0a3224dd565797febaf19385cd3584753f89bb44afe841e37c78";
    };
    aarch64-linux = {
      target = "aarch64-unknown-linux-musl";
      platform = "linux-aarch64";
      sha256 = "5f608711dc305dc936f728285e2d74186e486572b3068b82810ae2f88bc454a7";
    };
    aarch64-darwin = {
      target = "aarch64-apple-darwin";
      platform = "darwin-aarch64";
      sha256 = "9a5b4227999ba00ebf87c837a3760ebfdae3417c550abd7f547d2ea84bb3ffde";
    };
  };

  execBuild = execBuilds.${pkgs.stdenv.hostPlatform.system};

  exec =
    pkgs.runCommand "dprint-plugin-exec"
    {
      zip = fetchurl {
        url = "https://plugins.dprint.dev/dprint/dprint-plugin-exec/0.9.0/asset/dprint-plugin-exec-${execBuild.target}.zip";
        inherit (execBuild) sha256;
      };

      manifest = builtins.toJSON {
        schemaVersion = 2;
        kind = "process";
        name = "dprint-plugin-exec";
        version = "0.9.0";
        ${execBuild.platform} = {
          reference = "./plugin.zip";
          checksum = execBuild.sha256;
        };
      };

      passAsFile = ["manifest"];
    }
    ''
      mkdir -p $out
      cp $zip $out/plugin.zip
      cp $manifestPath $out/plugin.json
      echo "$out/plugin.json@$(sha256sum $out/plugin.json | cut -d' ' -f1)" > $out/reference
    '';

  formatters = [
    (workspace.toolchainFor {components = ["rustfmt"];})
    pkgs.ktlint
  ];
in
  pkgs.writeShellScriptBin "dprint" ''
    export PATH=${lib.makeBinPath formatters}:$PATH
    case "''${1-}" in
      fmt | check | lsp | editor-service | output-file-paths | output-resolved-config | output-format-times)
        command=$1
        shift
        exec ${pkgs.dprint}/bin/dprint "$command" "$@" --plugins \
          ${lib.concatStringsSep " " wasmPlugins} \
          "$(< ${exec}/reference)"
        ;;
      *)
        exec ${pkgs.dprint}/bin/dprint "$@"
        ;;
    esac
  ''
