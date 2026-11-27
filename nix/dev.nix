# Every process is disabled by default, so `up X` starts only X and its
# dependencies.
#
# nix does not build our code here. cargo, wasm-bindgen, stylance and bun
# build incrementally in the working tree. A *-build process reruns when the
# source files of its package change, and copies its output to dist/ only if
# the output changed. The process that uses the output restarts when dist/
# changes.
{
  pkgs,
  process-compose-flake,
  shell,
  defaults,
  # Each of these exports its processes as `dev`.
  parts,
}: let
  inherit (pkgs) lib;

  # Override one with <NAME>_PORT in the environment, for example TETHER_PORT
  # for tether.
  chosen = {
    tether = 5175;
    server = 5176;
    playwright = 5177;
  };

  variableOf = name:
    lib.toUpper (
      lib.concatMapStrings (letter:
        if letter != lib.toLower letter
        then "_${letter}"
        else letter) (
        lib.stringToCharacters name
      )
    )
    + "_PORT";

  # Commands use `${ports.tether}` rather than a literal "$TETHER_PORT". A
  # misspelt attribute fails evaluation, while a misspelt shell variable
  # silently expands to nothing.
  ports = lib.mapAttrs (name: _: "$" + variableOf name) chosen;

  theme = "Catppuccin Mocha";

  helpers =
    import ./dev/helpers.nix {inherit pkgs;}
    // {
      inherit ports defaults;
    };

  processes = lib.mergeAttrsList (map (part: part.dev helpers) parts);

  # Sets up what the dev shell would, so that the programs below also work
  # outside it. On macOS the linker comes from Xcode, so stdenv.cc is only
  # added on Linux.
  environment = ''
    cd "$(git rev-parse --show-toplevel)"
    export GIT_ROOT="$PWD"

    export PATH=${
      lib.makeBinPath (
        shell.tools ++ [pkgs.git] ++ lib.optional pkgs.stdenv.hostPlatform.isLinux pkgs.stdenv.cc
      )
    }:$PATH
    ${lib.concatStrings (
      lib.mapAttrsToList (name: value: ''
        export ${name}=${lib.escapeShellArg value}
      '')
      shell.variables
    )}
    ${lib.concatStrings (
      lib.mapAttrsToList (name: port: ''
        export ${variableOf name}="''${${variableOf name}:-${toString port}}"
      '')
      chosen
    )}
  '';

  compose = (import process-compose-flake.lib {inherit pkgs;}).makeProcessCompose {
    name = "tether-dev-compose";

    modules = [
      {
        defaults.processSettings = {
          # process-compose has `watch`, but process-compose-flake does not
          # declare an option for it yet.
          options.watch = lib.mkOption {
            type = lib.types.nullOr lib.types.attrs;
            default = null;
          };

          config.disabled = lib.mkDefault true;
        };

        settings = {
          inherit processes;
          is_strict = true;
        };

        # process-compose's own HTTP API is not used, and its default port is a
        # common one.
        cli.environment.PC_NO_SERVER = true;

        # Without this the TUI lists every process, and most are disabled.
        cli.environment.PC_HIDE_DISABLED_PROC = true;

        cli.preHook = ''
          ${environment}
          # Other subcommands reject --theme.
          if [ "''${1:-up}" = up ]; then
            set -- "$@" --theme ${lib.escapeShellArg theme}
          fi
        '';
      }
    ];
  };

  # Runs every *-build process once and then every *-setup process, in the
  # working tree, without process-compose or its watching. The setups link what
  # the builds left in dist/, so they come last.
  build = let
    run = suffix:
      lib.concatStrings (
        lib.mapAttrsToList (name: process: ''
          echo "==> ${name}" >&2
          ${lib.getExe process.command}
        '') (lib.filterAttrs (name: _: lib.hasSuffix suffix name) processes)
      );
  in
    pkgs.writeShellScriptBin "tether-dev-build" ''
      set -euo pipefail
      ${environment}
      ${run "-build"}
      ${run "-setup"}
    '';
in
  pkgs.symlinkJoin {
    name = "tether-dev";
    paths = [compose build];
    meta.mainProgram = "tether-dev-compose";
  }
