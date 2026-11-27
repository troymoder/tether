default:
    @just --list

# The dev processes (nix/dev.nix): `just dev up web`.
dev *args:
    tether-dev-compose {{args}}

# Builds everything `just dev` would, once, into dist/.
build:
    tether-dev-build

fmt:
    dprint fmt

check *args:
    nix flake check {{args}}

test: test-rust test-ui

test-rust:
    cargo nextest run

# Needs `just dev up playwright` running, which serves the page the tests load.
test-ui:
    cd crates/web && bunx playwright test

# Leaves .cache, which holds dev's tetherd data and emulators.
clean:
    cargo clean
    rm -rf dist result node_modules crates/web/{generated,node_modules} crates/web/tests/dist
