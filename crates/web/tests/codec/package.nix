{
  workspace,
  targets,
  wasm-bindgen-cli,
}: let
  wasm = targets.wasm32-unknown-unknown;

  cargo = wasm.cargo "test-codec";

  # Shared by the nix build and dev.
  bindgen = output: to: ''
    wasm-bindgen --target nodejs --out-name codec --out-dir ${to} ${output}/test_codec.wasm
  '';

  test-codec = wasm.craneLib.mkCargoDerivation (
    wasm.args
    // {
      pname = "tether-test-codec";
      cargoArtifacts = wasm.artifacts;
      src = workspace.sourceFor "web/tests/codec";
      doInstallCargoArtifacts = false;
      nativeBuildInputs = [wasm-bindgen-cli];
      buildPhaseCargoCommand = cargo.nix;
      installPhaseCommand = bindgen wasm.output.nix "$out";
    }
  );
in {
  inherit test-codec;

  # See nix/dev.nix. The argument is nix/dev/helpers.nix.
  dev = {
    built,
    out,
    ...
  }:
    built "test-codec" {
      command = ''
        ${cargo.dev}
        rm -rf target/tmp/codec
        ${bindgen wasm.output.dev "target/tmp/codec"}
      '';
      from = "target/tmp/codec";
      to = out "codec";
      packages = [test-codec];
    };
}
