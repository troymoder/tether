# Development

Everything runs from the nix dev shell: `direnv allow`, or `nix develop`.

## Dev loop

```sh
just dev up web       # the page, tetherd and everything they are built from
just dev up server    # tetherd alone
just build            # the same builds, once, without watching
just clean            # removes build outputs, and keeps tetherd's dev data
```

Open the page on the `tether` port. Changes rebuild and reload by themselves. The processes and
their ports are in [nix/dev.nix](../nix/dev.nix). `just` lists every recipe.

Dev's tetherd keeps its data in `.cache/` and reads `.cache/config.toml`, which is yours to edit.
The settings are the flags in [crates/server/src/settings.rs](../crates/server/src/settings.rs).

## Checks

| What                | How              |
| ------------------- | ---------------- |
| Rust tests          | `just test-rust` |
| The page, in Chrome | `just test-ui`   |
| Both                | `just test`      |
| Formatting          | `just fmt`       |
| Everything, as CI   | `just check`     |

`just test-ui` needs `just dev up playwright` running, which serves the page the tests load.

The page only builds for WebAssembly, so its tests are the Playwright suite in
[crates/web/tests](../crates/web/tests). They need no tetherd and no device.

`just check` is `nix flake check`, which also runs clippy, TypeScript and the lock file checks. Run
one check with `nix build .#checks.<system>.<name>`.

## Building

```sh
nix build
./result/bin/tetherd <port>
```

The flake's own outputs accept the Android SDK's licence. The modules and the overlay build from the
`pkgs` of whoever uses them, so they accept it in their own nixpkgs config.

## Where the build lives

Each component has a `package.nix` next to its source, which holds its packages, checks and dev
processes. `nix/` holds what they share, and [flake.nix](../flake.nix) wires them together.

## After changing dependencies

- **Rust:** run `cargo hakari generate`.
- **`crates/web/package.json`:** run `bun install`, then update the `nodeModules` hash in
  [crates/web/package.nix](../crates/web/package.nix).
- **wasm-bindgen or stylance:** the crate in `Cargo.lock` and the CLI in nix must be the same
  version.
