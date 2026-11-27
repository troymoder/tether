{
  pkgs,
  android,
}: let
  inherit (pkgs) lib;
  sdk = android.tools;

  # The nix build and the dev process share these commands. R8 strips
  # Kotlin's standard library to what the helper uses, so the page can carry
  # the result. Writing to a folder leaves out the library's resource files,
  # which the device does not need.
  build = {
    source,
    work,
    output,
  }: let
    platform = "${sdk}/libexec/android-sdk/platforms/android-36/android.jar";
    rules = pkgs.writeText "helper.pro" ''
      -keep class tether.Main { public static void main(java.lang.String[]); }
      # Kotlin's standard library marks its code with these, and nothing
      # reads them as it runs.
      -dontwarn org.jetbrains.annotations.**
    '';
  in ''
    export PATH=${
      lib.makeBinPath [
        pkgs.kotlin
        pkgs.jdk_headless
      ]
    }:$PATH
    # Compiled against android.jar alone, as the device has no JDK.
    kotlinc ${source} -no-jdk -no-reflect -jvm-target 1.8 \
      -classpath ${platform} -d ${work}/classes.jar
    java -cp ${sdk}/libexec/android-sdk/build-tools/*/lib/d8.jar com.android.tools.r8.R8 \
      --release --min-api 21 --lib ${platform} --pg-conf ${rules} \
      --output ${work} ${work}/classes.jar ${pkgs.kotlin}/lib/kotlin-stdlib.jar
    cp ${work}/classes.dex ${output}
  '';

  helper =
    pkgs.runCommand "tether-helper.dex"
    {
      # Rooted at the repository root, because `sourcesOf` (nix/dev/helpers.nix)
      # reads paths relative to it.
      src = lib.fileset.toSource {
        root = ../..;
        fileset = ./helper;
      };
    }
    (build {
      source = "$src/crates/android/helper/Main.kt";
      work = ".";
      output = "$out";
    });
in {
  inherit helper;

  # The argument is nix/dev/helpers.nix.
  dev = {
    built,
    out,
    ...
  }:
    built "android-helper" {
      command = ''
        mkdir -p target/tmp/android-helper
        ${build {
          source = "crates/android/helper/Main.kt";
          work = "target/tmp/android-helper";
          output = "target/tmp/android-helper/helper.dex";
        }}
      '';
      from = "target/tmp/android-helper/helper.dex";
      to = "${out "android-helper"}/helper.dex";
      packages = [helper];
    };
}
