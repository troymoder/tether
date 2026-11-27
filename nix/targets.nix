# The dependencies are built once per Rust target and shared by every package
# built for that target. This works because cargo-hakari gives each third-party
# crate the same features in every build.
{
  pkgs,
  workspace,
}: let
  inherit (pkgs) lib;

  # rust-toolchain.toml only lists the cross targets. The host target is always
  # available.
  hostTriple = pkgs.stdenv.hostPlatform.rust.rustcTarget;
  listed = (lib.importTOML ../rust-toolchain.toml).toolchain.targets;

  # crates/vcam's build script reads the kernel's V4L2 header with bindgen, on
  # Linux. glibc's headers have the kernel's in them, and libclang has its own
  # builtin ones.
  bindgen = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
    BINDGEN_EXTRA_CLANG_ARGS = "-isystem ${lib.getDev pkgs.glibc}/include";
  };

  # zstd, which packs the pictures the page sends for a camera, is C. nix's
  # wrapped compiler only builds for this machine, so the page gets the
  # unwrapped clang.
  wasmCompiler = {
    CC_wasm32_unknown_unknown = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
    AR_wasm32_unknown_unknown = "${pkgs.llvmPackages.llvm}/bin/llvm-ar";
  };

  variableOf = triple: name: "CARGO_TARGET_${lib.toUpper (lib.replaceStrings ["-"] ["_"] triple)}_${name}";

  # The host is linked with wild. A target's rustflags replace
  # .cargo/config.toml's, so its web-sys flag is given again.
  flags = extra: lib.concatStringsSep " " (["--cfg=web_sys_unstable_apis"] ++ extra);

  # On Linux, through nix's wrapped clang, which gives the binary NixOS's
  # dynamic linker and libraries, and the wrapped wild.
  hostLinker = lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
    ${variableOf hostTriple "LINKER"} = "${pkgs.clang}/bin/clang";
    ${variableOf hostTriple "RUSTFLAGS"} = flags ["-Clink-arg=--ld-path=${pkgs.wild}/bin/wild"];
  };

  overrides = {
    # Tests run on the host, so the host build also compiles the
    # dev-dependencies.
    ${hostTriple} = {
      args = {doCheck = true;} // bindgen // hostLinker;
      built = {
        dev = "--release";
        output.nix = "target/release";
        output.dev = "target/release";
      };
    };

    wasm32-unknown-unknown = {
      # So that the wasm clippy check can reuse this dependency build.
      components = ["clippy"];
      # rust-lld links it. wild 0.11 fails on the exports wasm-bindgen's crates
      # ask for (`symbol exported via --export not found`). The page builds
      # with web-release (crates/web/package.nix), and dependencies built with
      # any other profile would be compiled again.
      args = wasmCompiler // {CARGO_PROFILE = "web-release";};
      # Dev builds with web-dev into target/web, as .cargo/web-dev.toml says,
      # so its builds do not evict the host's in target/.
      built = {
        dev = "--config .cargo/web-dev.toml --profile web-dev";
        output.nix = "target/wasm32-unknown-unknown/web-release";
        output.dev = "target/web/wasm32-unknown-unknown/web-dev";
      };
    };
  };

  target = triple: let
    override = overrides.${triple} or {};
  in
    rec {
      inherit triple;

      craneLib = workspace.craneLibFor {
        targets = [triple];
        components = override.components or [];
      };

      args =
        workspace.cargoArgs
        // {
          pname = "tether-${triple}";
          doCheck = false;
        }
        # The host build names no target. With one, cargo would compile build
        # scripts and proc macros separately from the rest and nothing would be
        # shared.
        // lib.optionalAttrs (triple != hostTriple) {CARGO_BUILD_TARGET = triple;}
        // (override.args or {});

      artifacts = craneLib.buildDepsOnly (args // {inherit (workspace) dummySrc;});
    }
    // lib.optionalAttrs (override ? built) {
      # `flags` is for crane's cargoExtraArgs, `nix` for a derivation that
      # runs cargo itself, and `dev` for the working tree. Dev leaves out
      # --locked, so that changing a dependency updates Cargo.lock.
      cargo = package: {
        flags = "--locked -p ${package}";
        nix = "cargoWithProfile build --locked -p ${package}";
        dev = "cargo build ${override.built.dev} -p ${package}";
      };

      inherit (override.built) output;
    };
  all = lib.genAttrs ([hostTriple] ++ listed) target;
in
  all
  // {
    host = all.${hostTriple};

    # For the dev shell, which builds both.
    variables = bindgen // wasmCompiler // hostLinker;
  }
