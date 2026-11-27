{
  pkgs,
  workspace,
  targets,
  android,
  web,
  vcam,
  helper,
}: let
  inherit (pkgs) lib;
  inherit (targets) host;

  cargo = host.cargo "tetherd";

  server = host.craneLib.buildPackage (
    host.args
    // android.protos
    // {
      pname = "tetherd";
      cargoArtifacts = host.artifacts;
      # Only tetherd and the crates it depends on, so that a change to any
      # other crate does not rebuild it.
      src = workspace.sourceFor "server";
      # crane reads the manifest for a description. `src` is a build output,
      # and reading from one is a build during evaluation, which
      # `nix flake show` refuses.
      cargoToml = ../../Cargo.toml;
      cargoExtraArgs = cargo.flags;
      # The flake's `tests` check runs the tests of the whole workspace once.
      doCheck = false;
      meta.mainProgram = "tetherd";
    }
  );

  # The wrapper passes its own config file first, so a later `--config` or a
  # flag overrides it. It has no Android SDK, so it runs no emulators, unless
  # `sdk` or the `android_sdk` setting gives it one: an `androidsdk` from
  # nixpkgs' androidenv with the emulator and a system image.
  default = lib.makeOverridable (
    {sdk ? null}:
      pkgs.runCommand "tetherd"
      {
        nativeBuildInputs = [pkgs.makeWrapper];
        meta.mainProgram = "tetherd";
      }
      ''
        makeWrapper ${server}/bin/tetherd $out/bin/tetherd \
          --add-flags "--config ${
          android.defaults sdk (
            {
              web = "${web}";
              android_helper = "${helper}";
            }
            // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {vcam = "${vcam}/lib/libvcam.so";}
          )
        }" \
          ${
          # setsid keeps an emulator's X server out of tetherd's process group.
          lib.optionalString pkgs.stdenv.hostPlatform.isLinux
          "--prefix PATH : ${lib.makeBinPath [pkgs.util-linux]}"
        }
      ''
  ) {};
in {
  inherit server default;

  dev = {
    step,
    built,
    out,
    completed,
    ports,
    defaults,
    ...
  }: let
    linux = pkgs.stdenv.hostPlatform.isLinux;
    binary = "${out "server"}/tetherd";
  in
    built "server" {
      command = cargo.dev;
      from = "${host.output.dev}/tetherd";
      to = binary;
      packages = [server];
    }
    // {
      # systemfd holds the listening socket across restarts, so the page's
      # requests wait instead of being refused. --no-pid because tetherd is not
      # systemfd's own child. watchexec restarts tetherd inside systemfd,
      # because process-compose's own watch would drop the socket. The stop
      # timeout leaves room for the emulators to save their snapshots. The shim
      # is passed by full path because the emulators load it through LD_PRELOAD.
      server = {
        command = step "server" ''
          touch .cache/config.toml
          exec systemfd --no-pid --socket "http::127.0.0.1:${ports.server}" -- \
            watchexec --restart --no-vcs-ignore --debounce 500ms --watch "${binary}" \
            --stop-signal SIGTERM --stop-timeout 70s \
            -- "exec ${binary} --config ${defaults} --config .cache/config.toml \
            --android-helper ${out "android-helper"}/helper.dex \
            ${lib.optionalString linux "--vcam ${out "vcam"}/libvcam.so"} \
            --origin localhost:${ports.tether} --origin 127.0.0.1:${ports.tether} ${ports.server}"
        '';

        depends_on =
          {
            server-build = completed;
            android-helper-build = completed;
          }
          // lib.optionalAttrs linux {vcam-build = completed;};
      };
    };
}
