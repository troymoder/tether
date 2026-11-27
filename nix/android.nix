# The SDK is unfree and has a licence to accept. nixpkgs builds it only for a
# `pkgs` whose config does both, which flake.nix decides.
{pkgs}: rec {
  compose = extra:
    (pkgs.androidenv.composeAndroidPackages (
      {
        includeEmulator = true;
        cmdLineToolsVersion = "8.0";
        platformVersions = ["36"];
      }
      // extra
    )).androidsdk;

  # The files the builds read: android.jar and R8 for the helper, and the
  # emulator's protos for tetherd. It has no system image, which is most of
  # an SDK's size, so building the package does not download one.
  tools = compose {};

  # The SDK dev's tetherd runs emulators from. The packaged tetherd has none
  # unless it is given one (crates/server/package.nix). The image is for the
  # host's architecture, because emulating another one is too slow to use.
  sdk = compose {
    includeSystemImages = true;
    systemImageTypes = ["google_apis"];
    abiVersions = [
      (
        if pkgs.stdenv.hostPlatform.isAarch64
        then "arm64-v8a"
        else "x86_64"
      )
    ];
  };

  # tetherd's build.rs (crates/server/build.rs) generates a gRPC client from
  # emulator_controller.proto in this folder.
  protos = {
    EMULATOR_PROTOS = "${tools}/libexec/android-sdk/emulator/lib";
  };

  # crates/android/src/scrcpy.rs speaks this version's protocol.
  scrcpyVersion = "5.0.1";
  scrcpyServer = pkgs.fetchurl {
    url = "https://github.com/Genymobile/scrcpy/releases/download/v${scrcpyVersion}/scrcpy-server-v${scrcpyVersion}";
    hash = "sha256-dk6295gR1SEf6d80ESCIK6mZTHphuJfXvz+2YuU7xTY=";
  };

  defaults = sdk: extra:
    (pkgs.formats.toml {}).generate "tetherd.toml" (
      pkgs.lib.optionalAttrs (sdk != null) {android_sdk = "${sdk}";}
      // {
        scrcpy_server = "${scrcpyServer}";
        ffmpeg = pkgs.lib.getExe pkgs.ffmpeg-headless;
      }
      // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
        xwayland_satellite = pkgs.lib.getExe pkgs.xwayland-satellite;
      }
      // extra
    );
}
