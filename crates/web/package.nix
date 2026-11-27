{
  pkgs,
  workspace,
  targets,
  test-codec,
}: let
  inherit (pkgs) lib;
  fs = lib.fileset;

  wasm = targets.wasm32-unknown-unknown;
  inherit (wasm) craneLib;

  cargo = wasm.cargo "web";

  # Has to be the version of the wasm-bindgen crate in Cargo.lock.
  wasm-bindgen-cli = pkgs.wasm-bindgen-cli_0_2_130;

  assets = fs.unions [
    ./package.json
    ./bun.lock
    ./rspack.config.ts
    ./tsconfig.json
    ./page.html
    ./glue
    ./main.scss
    ./_theme.scss
    ./base.scss
  ];

  # Shared by the nix build and dev. Run at the repository root.
  generate = {
    module,
    generated,
    work,
    optimise ? false,
  }: ''
    rm -rf "${generated}" "${work}"
    mkdir -p "${generated}" "${work}"
    ${
      if optimise
      then ''
        wasm-bindgen --target web --out-dir "${generated}" --out-name web "${module}"
        wasm-opt -Oz --strip-debug --strip-producers "${generated}/web_bg.wasm" -o "${generated}/web_bg.wasm"
      ''
      else ''
        wasm-bindgen --target web --out-dir "${generated}" --out-name web --split-debug-info "${module}" --debug-info-url web_bg.debug.wasm
      ''
    }
    stylance crates/web --output-dir "${work}"
    (
      cd crates/web
      ${lib.getExe pkgs.dart-sass} --load-path . --load-path "${work}" \
        main.scss "${generated}/main.css"
    )
  '';

  # Bun runs rspack, as the CLI's `#!/usr/bin/env node` is missing in the
  # sandbox.
  rspack = "bun node_modules/@rspack/cli/bin/rspack.js";
  bundle = out: ''
    (cd crates/web && ${rspack} build -o "${out}")
  '';

  # rspack strips TypeScript's types without checking them, so tsc checks each
  # project. The glue needs wasm-bindgen's output, so the page's build checks it.
  typecheck = projects: ''
    (cd crates/web && for project in ${lib.escapeShellArgs projects}; do
      bun node_modules/typescript/bin/tsc -p "$project"
    done)
  '';

  # Nothing can be fetched while the page builds. After a bun.lock change, set
  # the hash to "" and build this to be told the new one.
  nodeModules = pkgs.stdenvNoCC.mkDerivation {
    name = "tether-web-node-modules";
    src = fs.toSource {
      root = ./.;
      fileset = fs.unions [
        ./package.json
        ./bun.lock
      ];
    };
    nativeBuildInputs = [pkgs.bun];
    dontConfigure = true;
    dontFixup = true;
    buildPhase = ''
      export HOME=$TMPDIR SSL_CERT_FILE=${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt
      bun install --frozen-lockfile --no-progress --ignore-scripts
    '';
    # In a folder of that name, as bun finds a package's dependencies by
    # following the link to it and looking for one.
    installPhase = "mkdir $out && cp -r node_modules $out/";
    outputHashMode = "recursive";
    outputHashAlgo = "sha256";
    outputHash = "sha256-Cg1xryf+Ahgk3z7bt+AhIAeC6rV24Qe3VmbDlruX9Ys=";
  };

  args =
    wasm.args
    // {
      pname = "tether-web";
      cargoArtifacts = wasm.artifacts;
      src = workspace.sourceWith [assets];
      cargoExtraArgs = cargo.flags;
    };

  web = craneLib.mkCargoDerivation (
    args
    // {
      doInstallCargoArtifacts = false;
      nativeBuildInputs = [
        wasm-bindgen-cli
        pkgs.binaryen
        pkgs.bun
        pkgs.stylance-cli
        pkgs.brotli
        pkgs.zstd
      ];
      buildPhaseCargoCommand = ''
        ${cargo.nix}
        export HOME=$TMPDIR
        ln -s ${nodeModules}/node_modules crates/web/node_modules
        ${generate {
          module = "${wasm.output.nix}/web.wasm";
          generated = "$TMPDIR/web/generated";
          work = "$TMPDIR/web/styles";
          optimise = true;
        }}
        ln -s "$TMPDIR/web/generated" crates/web/generated
        ${typecheck ["glue" "glue/serviceWorker" "glue/worker"]}
        ${bundle "$out"}
      '';
      installPhaseCommand = ''
        find $out -type f \( -name '*.wasm' -o -name '*.js' -o -name '*.css' -o -name '*.html' \) \
          -exec gzip -9 -k {} \; \
          -exec brotli -q 11 -k {} \; \
          -exec zstd -19 -q -k {} \;
      '';
    }
  );

  # miniserve hands out `web` on the port tests/tetherd.ts looks for the page
  # at, and the codec goes where tests/protocol.ts imports it from.
  playwright-report =
    pkgs.runCommand "tether-playwright"
    {
      nativeBuildInputs = [pkgs.bun pkgs.curl pkgs.util-linux pkgs.iproute2];
      src = fs.toSource {
        root = ../..;
        fileset = fs.unions [
          ./package.json
          ./playwright.config.ts
          ./tests
        ];
      };
      PLAYWRIGHT_BROWSERS_PATH = pkgs.playwright-driver.browsers;
      PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
      PLAYWRIGHT_SKIP_DOWNLOAD = "1";
      FONTCONFIG_FILE = pkgs.makeFontsConf {fontDirectories = [pkgs.dejavu_fonts];};
    }
    ''
      export HOME=$TMPDIR
      cp -r --no-preserve=mode $src/. .

      cd crates/web

      mkdir -p tests/dist
      ln -s ${test-codec} tests/dist/codec
      ln -s ${nodeModules}/node_modules node_modules
      mkdir -p "$out"

      unshare -rn ${pkgs.writeShellScript "playwright" ''
        # Each command in the log, as some fail without a word, such as a
        # script whose `#!` names a program the sandbox lacks.
        set -euxo pipefail

        # Chrome's WebRTC skips loopback interfaces and treats a network without
        # a default route as offline, so the sandbox's network has neither
        # host candidate. A dummy interface with a default route gives it one.
        ip link set lo up
        ip link add tests type dummy
        ip addr add 10.11.0.1/24 dev tests
        ip link set tests up
        ip route add default dev tests

        ${lib.getExe pkgs.miniserve} --quiet --spa --index index.html -i 127.0.0.1 --port 5177 ${web} &
        server=$!
        trap 'kill $server' EXIT

        for _ in $(seq 5); do
          curl -sf http://127.0.0.1:5177/ >/dev/null && break
          sleep 0.1
        done
        curl -sf http://127.0.0.1:5177/ >/dev/null || { echo "miniserve didn't start"; exit 1; }

        bun node_modules/@playwright/test/cli.js test --reporter=list,html --workers=100% --output $out || touch $out/failed
      ''} || { echo "playwright failed: exit $?"; touch $out/failed; }
    '';
in {
  inherit web playwright-report nodeModules wasm-bindgen-cli;

  checks =
    {
      clippy-web = craneLib.cargoClippy (
        args
        // {
          src = workspace.sourceFor "web";
          cargoClippyExtraArgs = "-- --deny warnings";
        }
      );

      typescript =
        pkgs.runCommand "tether-typescript"
        {
          nativeBuildInputs = [pkgs.bun];
          src = fs.toSource {
            root = ../..;
            fileset = fs.unions [
              ./package.json
              ./tsconfig.json
              ./rspack.config.ts
              ./playwright.config.ts
              ./tests
            ];
          };
        }
        ''
          export HOME=$TMPDIR
          cp -r --no-preserve=mode $src/. .
          ln -s ${nodeModules}/node_modules crates/web/node_modules
          mkdir -p crates/web/tests/dist
          ln -s ${test-codec} crates/web/tests/dist/codec
          ${typecheck ["." "tests"]}
          mkdir -p $out
        '';
    }
    # The report's network needs Linux's namespaces.
    // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
      playwright = let
        drv = builtins.unsafeDiscardStringContext playwright-report.drvPath;
      in
        pkgs.runCommand "tether-playwright" {} ''
          if [ -e ${playwright-report}/failed ]; then
            echo "tests failed: nix log ${drv}" >&2
            exit 1
          fi
          mkdir -p $out
        '';
    };

  # The argument is nix/dev/helpers.nix.
  dev = {
    step,
    built,
    out,
    linked,
    started,
    completed,
    ports,
    ...
  }: let
    serve = flags: ''
      cd crates/web
      ${rspack} serve --host 127.0.0.1 ${flags}
    '';
  in
    built "web" {
      command = ''
        ${cargo.dev}
        [ -e crates/web/node_modules ] || (cd crates/web && bun install --frozen-lockfile)

        ${generate {
          module = "${wasm.output.dev}/web.wasm";
          generated = "$GIT_ROOT/target/tmp/web/generated";
          work = "$GIT_ROOT/target/tmp/web/styles";
        }}
      '';
      from = "target/tmp/web/generated";
      to = out "web";
      packages = [
        {src = workspace.sourceFor "web";}
        {
          src = fs.toSource {
            root = ../..;
            fileset = assets;
          };
        }
      ];
    }
    // {
      web = {
        command = step "web" (serve ''--port "${ports.tether}" --env proxy="http://127.0.0.1:${ports.server}"'');

        depends_on = {
          web-setup = completed;
          server = started;
        };
      };

      # The tests' routes answer the page's signalling, so this needs no tetherd.
      playwright = {
        command = step "playwright" ''
          ${serve ''--port "${ports.playwright}"''}
        '';

        depends_on = {
          web-setup = completed;
          playwright-setup = completed;
        };
      };

      web-setup = {
        command = step "web-setup" ''
          ${linked (out "web") "crates/web/generated"}
        '';

        depends_on.web-build = completed;
      };

      playwright-setup = {
        command = step "playwright-setup" ''
          ${linked (out "codec") "crates/web/tests/dist/codec"}
        '';

        depends_on.test-codec-build = completed;
      };
    };
}
