{
  workspace,
  targets,
}: let
  inherit (targets) host;

  cargo = host.cargo "vcam";

  vcam = host.craneLib.buildPackage (
    host.args
    // {
      pname = "vcam";
      cargoArtifacts = host.artifacts;
      # Only vcam and its dependencies, so a change to another crate does not
      # rebuild it.
      src = workspace.sourceFor "vcam";
      # crane reads the manifest for a description. `src` is a build output,
      # and reading from one is a build during evaluation, which
      # `nix flake show` refuses.
      cargoToml = ../../Cargo.toml;
      cargoExtraArgs = cargo.flags;
      # The shim only works inside an emulator.
      doCheck = false;
    }
  );
in {
  inherit vcam;

  # The argument is nix/dev/helpers.nix. A running emulator picks up a new
  # build only when it restarts.
  dev = {
    built,
    out,
    ...
  }:
    built "vcam" {
      command = cargo.dev;
      from = "${host.output.dev}/libvcam.so";
      to = "${out "vcam"}/libvcam.so";
      packages = [vcam];
    };
}
