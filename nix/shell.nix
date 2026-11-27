{
  pkgs,
  workspace,
  targets,
  android,
  # Has to be the exact wasm-bindgen version in Cargo.lock.
  wasm-bindgen-cli,
  defaults,
  dev,
}: let
  variables =
    {
      # cargo-llvm-cov needs the llvm tools of the llvm rustc was built with.
      LLVM_COV = "${workspace.llvmTools}/llvm-cov";
      LLVM_PROFDATA = "${workspace.llvmTools}/llvm-profdata";

      RUST_SRC_PATH = workspace.rustSrc;

      # crates/web/tests pins @playwright/test in package.json to the version
      # of these browsers.
      PLAYWRIGHT_BROWSERS_PATH = "${pkgs.playwright-driver.browsers}";
      PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
      PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";

      RUSTC_WRAPPER = pkgs.lib.getExe pkgs.sccache;
    }
    // targets.variables
    // android.protos;

  tools =
    [
      workspace.devToolchain
      pkgs.cargo-nextest
      pkgs.cargo-llvm-cov
      # Run `cargo hakari generate` after changing a dependency. It keeps
      # crates/workspace-hack up to date.
      pkgs.cargo-hakari
      (import ./dprint.nix {inherit pkgs workspace;})
      pkgs.mergiraf
      pkgs.just
      pkgs.nixd

      wasm-bindgen-cli
      pkgs.binaryen # wasm-opt
      pkgs.bun
      # Has to be the version of the stylance crate in Cargo.lock.
      pkgs.stylance-cli

      # tetherd under dev: systemfd holds its socket while watchexec restarts it.
      pkgs.watchexec
      pkgs.systemfd
      pkgs.sccache
      pkgs.wild
      pkgs.wasm-tools
      pkgs.libllvm
    ]
    # setsid keeps an emulator's X server out of tetherd's process group.
    ++ pkgs.lib.optional pkgs.stdenv.hostPlatform.isLinux pkgs.util-linux;
in
  pkgs.mkShell (
    variables
    // {
      # With `tether-dev-compose` on the path, starting it does not evaluate the
      # flake. `nix run .#dev` does on every start, which takes a few seconds.
      nativeBuildInputs = tools ++ [dev];

      # git reads merge drivers only from its config, which cannot be
      # committed. dev's builds write under GIT_ROOT (nix/dev/helpers.nix).
      shellHook = ''
        export GIT_ROOT=$(git rev-parse --show-toplevel)
        git config merge.mergiraf.name mergiraf
        git config merge.mergiraf.driver 'mergiraf merge --git %O %A %B -s %S -x %X -y %Y -p %P -l %L'
        mkdir -p "$(git rev-parse --git-common-dir)/info"
        mergiraf languages --gitattributes > "$(git rev-parse --git-common-dir)/info/attributes"

        mkdir -p "$GIT_ROOT/.cache"
        touch "$GIT_ROOT/.cache/config.toml"
        ln -sfn ${defaults} "$GIT_ROOT/.cache/defaults.toml"
      '';

      # For nix/dev.nix. It cannot read nativeBuildInputs, because that list
      # contains its own package.
      passthru = {inherit variables tools;};
    }
  )
