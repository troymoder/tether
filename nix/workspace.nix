{
  pkgs,
  crane,
  rust-overlay,
}: let
  inherit (pkgs) lib;
  fs = lib.fileset;

  devToolchain = (rust-overlay.lib.mkRustBin {} pkgs).fromRustupToolchainFile ../rust-toolchain.toml;

  # A toolchain with only the targets and components that one build needs, so
  # that it does not download the others.
  toolchainFor = {
    targets ? [],
    components ? [],
  }:
    devToolchain.override {
      inherit targets;
      extensions = components;
    };

  craneLibFor = wanted: (crane.mkLib pkgs).overrideToolchain (toolchainFor wanted);

  toolchain = toolchainFor {};
  craneLib = craneLibFor {};

  # The same crates as Cargo.toml's `members`.
  members =
    builtins.attrNames (
      lib.filterAttrs (_: type: type == "directory") (builtins.readDir ../crates)
    )
    ++ ["web/tests/codec"];

  manifests = lib.genAttrs members (name: lib.importTOML (../crates + "/${name}/Cargo.toml"));

  folderOf = lib.listToAttrs (
    map (folder: lib.nameValuePair manifests.${folder}.package.name folder) members
  );

  # dev-dependencies are left out because only the tests need them.
  localDependencies = folder: let
    manifest = manifests.${folder};
    tablesOf = section: [
      (section.dependencies or {})
      (section.build-dependencies or {})
    ];
    tables = tablesOf manifest ++ lib.concatMap tablesOf (lib.attrValues (manifest.target or {}));
    # A renamed dependency has its real name in `package`.
    packages =
      lib.concatMap (lib.mapAttrsToList (
        key: spec:
          if lib.isAttrs spec
          then spec.package or key
          else key
      ))
      tables;
  in
    map (package: folderOf.${package}) (lib.filter (package: folderOf ? ${package}) packages);

  cratesOf = folder:
    map (item: item.key) (
      builtins.genericClosure {
        startSet = [{key = folder;}];
        operator = item: map (key: {inherit key;}) (localDependencies item.key);
      }
    );

  # The page's components read their .module.scss as they compile
  # (stylance's `import_style!`).
  # A crate inside another, as the codec is in web, is left out of the outer
  # one, so changing it does not rebuild the page.
  crateFiles = name: let
    folder = ../crates + "/${name}";
    inner = lib.filter (member: lib.hasPrefix "${name}/" member) members;
  in
    fs.difference
    (fs.unions [
      (craneLib.fileset.commonCargoSources folder)
      (fs.fileFilter (file: lib.hasSuffix ".module.scss" file.name) (folder + "/src"))
    ])
    (fs.unions (map (member: ../crates + "/${member}") inner));

  sourceOf = extra: crates: stubbed:
    fs.toSource {
      root = ../.;
      fileset = fs.unions (
        [
          ../Cargo.toml
          ../Cargo.lock
          ../.cargo
        ]
        ++ extra
        ++ map crateFiles crates
        ++ map (name: ../crates + "/${name}/Cargo.toml") stubbed
      );
    };
in rec {
  inherit
    craneLib
    craneLibFor
    toolchain
    toolchainFor
    devToolchain
    ;

  # Points at the rust-src component directly and not through the toolchain,
  # where it is a symlink. nix reads a lockfile from here during evaluation, and
  # pure evaluation does not follow a symlink from one store path into another.
  rustSrc = "${toolchain.passthru.availableComponents.rust-src}/lib/rustlib/src/rust/library";

  llvmTools = "${
    toolchainFor {components = ["llvm-tools"];}
  }/lib/rustlib/${pkgs.stdenv.hostPlatform.rust.rustcTarget}/bin";

  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = sourceWith [];

  sourceWith = extra: sourceOf extra members [];

  # The source for building one crate: the crate and the workspace crates it
  # depends on. Every other member is replaced by an empty lib.rs, so that
  # changing it does not rebuild this crate. Its Cargo.toml stays, because cargo
  # needs the manifest of every workspace member to accept the lockfile.
  sourceFor = crate: let
    crates = cratesOf crate;
    stubbed = lib.subtractLists crates members;
    files = sourceOf [] crates stubbed;
  in
    # `files` is the same source without the stubs. nix/dev/helpers.nix lists it
    # to know which files to watch. It cannot list this derivation without building it.
    pkgs.runCommand "source" {passthru = {inherit files;};} ''
      cp -r ${files} $out
      chmod -R u+w $out
      ${lib.concatMapStrings (name: ''
          mkdir -p $out/crates/${name}/src
          touch $out/crates/${name}/src/lib.rs
        '')
        stubbed}
    '';

  cargoArgs = {
    pname = "tether";
    inherit version;
    strictDeps = true;
    cargoVendorDir = vendored;
  };

  vendored = craneLib.vendorCargoDeps {
    cargoLock = ../Cargo.lock;

    # yuv-sys's build script copies libyuv out of its source and rewrites the
    # copies in place. From the store they are read-only, so the build would
    # fail with "Permission denied", where cargo's own registry is writable.
    overrideVendorCargoPackage = package: drv:
      if package.name == "yuv-sys"
      then
        drv.overrideAttrs (old: {
          postPatch =
            (old.postPatch or "")
            + ''
              substituteInPlace build.rs --replace-fail \
                'fs::write(&file.path(), content.to_string()).unwrap();' \
                'let mut permissions = fs::metadata(file.path()).unwrap().permissions();
                 permissions.set_readonly(false);
                 fs::set_permissions(file.path(), permissions).unwrap();
                 fs::write(&file.path(), content.to_string()).unwrap();'
            '';
        })
      else drv;
  };

  # The aggressive filter strips the manifest fields that do not affect
  # dependency resolution, so that editing a description or a lint does not
  # rebuild the dependencies.
  dummySrc = craneLib.mkDummySrc {
    inherit src;
    cleanCargoTomlFilter = craneLib.filters.cargoTomlAggressive;
  };
}
