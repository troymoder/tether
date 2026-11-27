# tether

A web page for working with mobile devices during development. tetherd runs on the machine your
Android phones, emulators and iPhones are attached to, and serves a page that shows their screens
and manages their apps, files, logs and more from any browser.

## Run it

```sh
nix build
./result/bin/tetherd <port>
```

Open `http://localhost:<port>`. The first browser is asked for the setup code tetherd prints.

Building tetherd uses the Android SDK, which is unfree and has a licence. Building from this flake
directly accepts it.

## Install it

The flake has two modules, with the same options:

- **`nixosModules.default`** runs one tetherd for the machine, as a system service with its own
  user. Its setup code is in `journalctl -u tetherd`.
- **`homeModules.default`** runs one for you, as a user service. Its emulators draw with the GPU
  through your desktop session.

```nix
{
  imports = [tether.nixosModules.default]; # or tether.homeModules.default

  nixpkgs.config = {
    allowUnfree = true;
    android_sdk.accept_license = true;
  };

  services.tether = {
    enable = true;
    settings.port = 5175;
  };
}
```

The modules build tetherd from your `pkgs`, so the licence is yours to accept, as above. The same
goes for `tether.overlays.default`, which adds `pkgs.tetherd`. `tetherd --help` lists the other
settings.

- **iPhones** need a udev rule, which takes root. The NixOS service has its own. With home-manager
  on NixOS, import the NixOS module too and set `hardware.tether.enable` in place of
  `services.tether`. On another distribution, copy [nix/70-tether.rules](nix/70-tether.rules) to
  `/etc/udev/rules.d`.
- **Emulators** are off until `services.tether.android.enable` is set, which gives tetherd an
  Android SDK with one system image. `services.tether.android.sdk` takes your own. They also need
  KVM on Linux.
- **GPU emulators** under the NixOS service need `services.tether.gpu.enable`, which runs a display
  with no screen for them to draw through. Without it they render in software.

## Work on it

```sh
direnv allow    # or: nix develop
just dev up web
```

## Docs

- [Features](docs/features.md): what it does, and for which devices.
- [Architecture](docs/architecture.md): the parts and how they talk.
- [Development](docs/development.md): the dev loop, tests and builds.
- [Conventions](AGENTS.md): code style, comments, docs and commits.

## Licence

MIT or Apache-2.0, at your option.
