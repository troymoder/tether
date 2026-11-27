{
  pkgs,
  workspace,
  targets,
  android,
}: let
  inherit (workspace) craneLib;
  fs = pkgs.lib.fileset;

  checkArgs =
    targets.host.args
    # tetherd's build.rs reads the emulator's protos.
    // android.protos
    // {
      inherit (workspace.cargoArgs) pname;
      inherit (workspace) src;
      cargoArtifacts = targets.host.artifacts;
      # No later build uses a check's build output, so it is not saved.
      doInstallCargoArtifacts = false;
    };
in {
  # The page's tests only run in a browser.
  tests = craneLib.cargoNextest (
    checkArgs
    // {
      # crates/quicktime's tests read captured messages from here as they run.
      src = workspace.sourceWith [../crates/quicktime/fixtures];
      cargoNextestExtraArgs = "--locked --workspace --exclude web";
    }
  );

  clippy = (workspace.craneLibFor {components = ["clippy"];}).cargoClippy (
    checkArgs // {cargoClippyExtraArgs = "--workspace --exclude web --all-targets -- --deny warnings";}
  );

  # The shared dependency builds in targets.nix depend on workspace-hack being
  # up to date and on every crate depending on it. This checks both. Not
  # `cargo hakari verify`: with output-single-feature it counts every
  # single-feature dependency as a failure and names none.
  hakari = craneLib.mkCargoDerivation {
    inherit (workspace.cargoArgs) pname version cargoVendorDir;
    pnameSuffix = "-hakari";
    src = workspace.sourceWith [../.config/hakari.toml];
    cargoArtifacts = null;
    doInstallCargoArtifacts = false;
    nativeBuildInputs = [pkgs.cargo-hakari];

    buildPhaseCargoCommand = ''
      cargo hakari generate --diff
      cargo hakari manage-deps --dry-run
    '';
  };

  dprint =
    pkgs.runCommand "tether-dprint"
    {
      nativeBuildInputs = [(import ./dprint.nix {inherit pkgs workspace;})];
      src = ../.;
    }
    ''
      cd $src
      DPRINT_CACHE_DIR=$TMPDIR dprint check
      mkdir -p $out
    '';

  # Cargo.lock needs no check here, because every Rust build runs with
  # --locked. The source holds only the bun.lock files and their manifests, so
  # the check does not rerun for unrelated changes.
  locks =
    pkgs.runCommand "tether-locks"
    {
      nativeBuildInputs = [pkgs.bun];

      src = fs.toSource {
        root = ../.;
        fileset = fs.fileFilter (file: builtins.elem file.name ["bun.lock" "package.json"]) ../.;
      };
    }
    ''
      export HOME=$TMPDIR
      cp -r $src source && chmod -R u+w source && cd source

      for lock in $(find . -name bun.lock); do
        (cd $(dirname $lock) && bun install --frozen-lockfile --lockfile-only) || {
          echo "$lock does not match its manifest. Run bun install and commit the result." >&2
          exit 1
        }
      done

      mkdir -p $out
    '';
}
