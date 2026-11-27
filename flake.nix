{
  description = "A web page for working with mobile devices during development";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.rust-overlay = {
    url = "github:oxalica/rust-overlay";
    inputs.nixpkgs.follows = "nixpkgs";
  };
  inputs.crane.url = "github:ipetkov/crane";
  inputs.process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
  # Only the home-manager module's test uses it (nix/test.nix).
  inputs.home-manager = {
    url = "github:nix-community/home-manager";
    inputs.nixpkgs.follows = "nixpkgs";
  };

  # Every component has a package.nix next to its source. nix/ holds what they
  # share.
  outputs = {
    nixpkgs,
    rust-overlay,
    crane,
    process-compose-flake,
    home-manager,
    ...
  }: let
    inherit (nixpkgs) lib;

    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "aarch64-darwin"
    ];

    build = pkgs: let
      linux = pkgs.stdenv.hostPlatform.isLinux;

      workspace = import ./nix/workspace.nix {inherit pkgs crane rust-overlay;};
      targets = import ./nix/targets.nix {inherit pkgs workspace;};
      android = import ./nix/android.nix {inherit pkgs;};

      android-helper = import ./crates/android/package.nix {inherit pkgs android;};
      codec = import ./crates/web/tests/codec/package.nix {
        inherit workspace targets;
        inherit (web) wasm-bindgen-cli;
      };
      vcam = import ./crates/vcam/package.nix {inherit workspace targets;};
      web = import ./crates/web/package.nix {
        inherit pkgs workspace targets;
        inherit (codec) test-codec;
      };
      server = import ./crates/server/package.nix {
        inherit
          pkgs
          workspace
          targets
          android
          ;
        inherit (web) web;
        inherit (vcam) vcam;
        inherit (android-helper) helper;
      };

      checks = import ./nix/checks.nix {inherit pkgs workspace targets android;};

      defaults = android.defaults android.sdk {data = ".cache";};

      devShell = import ./nix/shell.nix {
        inherit
          pkgs
          workspace
          targets
          android
          defaults
          dev
          ;
        inherit (web) wasm-bindgen-cli;
      };

      dev = import ./nix/dev.nix {
        inherit pkgs process-compose-flake defaults;
        shell = devShell;
        parts =
          [
            android-helper
            codec
            web
            server
          ]
          # The camera shim is for Linux's V4L2 only.
          ++ lib.optional linux vcam;
      };
    in {
      packages =
        {
          inherit (server) default server;
          inherit (web) web nodeModules;
          inherit (android-helper) helper;
          inherit (codec) test-codec;
          inherit dev;
          # The SDK dev's tetherd runs emulators from, and the modules' default.
          android-sdk = android.sdk;
        }
        // lib.optionalAttrs linux {
          inherit (vcam) vcam;
          inherit (web) playwright-report;
        };

      checks =
        checks
        // web.checks
        // {inherit (server) server;}
        // lib.optionalAttrs linux {
          modules = import ./nix/test.nix {inherit pkgs modules home-manager;};
        };

      inherit devShell;

      formatter = pkgs.writeShellScriptBin "fmt" ''
        exec ${lib.getExe (import ./nix/dprint.nix {inherit pkgs workspace;})} fmt "$@"
      '';
    };

    # Building tetherd reads files from the Android SDK, which is unfree and
    # has a licence to accept. This flake's own outputs accept it, for the
    # people working on tether.
    developer = lib.genAttrs systems (
      system:
        build (
          import nixpkgs {
            inherit system;
            config = {
              allowUnfree = true;
              android_sdk.accept_license = true;
            };
          }
        )
    );

    # The modules and the overlay build from the `pkgs` of whoever uses them,
    # so that their nixpkgs config says what they accepted.
    packagesFor = pkgs:
      (build (
        import nixpkgs {
          inherit (pkgs.stdenv.hostPlatform) system;
          inherit (pkgs) config;
        }
      )).packages;

    modules = import ./nix/modules.nix packagesFor;
  in {
    nixosModules.default = modules.nixos;
    homeModules.default = modules.homeManager;
    overlays.default = final: _: {tetherd = (packagesFor final).default;};

    packages = lib.mapAttrs (_: built: built.packages) developer;

    checks = lib.mapAttrs (system: built:
      built.checks
      // {
        consumer-accepted =
          (packagesFor (import nixpkgs {
            inherit system;
            config = {
              allowUnfree = true;
              android_sdk.accept_license = true;
            };
          })).default;
      })
    developer;

    devShells = lib.mapAttrs (_: built: {default = built.devShell;}) developer;

    formatter = lib.mapAttrs (_: built: built.formatter) developer;
  };
}
