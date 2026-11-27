# tether

A web page for working with mobile devices during development. tetherd runs on the machine your
Android phones, emulators and iPhones are attached to, and serves a page that shows their screens
and manages their apps, files, logs and more from any browser.

## Install it

tether is a nix flake with a NixOS module and a home-manager module. Add it as an input:

```nix
inputs.tether.url = "github:troymoder/tether";
```

Both modules build tetherd from your `pkgs`. The build uses the Android SDK, which is unfree and has
a licence, so your nixpkgs config has to accept both:

```nix
nixpkgs.config = {
  allowUnfree = true;
  android_sdk.accept_license = true;
};
```

### NixOS

One tetherd for the machine, as a system service with its own user.

```nix
{inputs, ...}: {
  imports = [inputs.tether.nixosModules.default];

  services.tether = {
    enable = true;
    settings.port = 5175;
    android.enable = true; # emulators
    gpu.enable = true; # lets emulators draw with the GPU
  };
}
```

Open `http://localhost:5175`. The first browser is asked for a setup code, which is in
`journalctl -u tetherd`.

### home-manager

One tetherd for you, as a user service. Its emulators draw with the GPU through your desktop
session.

```nix
{inputs, ...}: {
  imports = [inputs.tether.homeModules.default];

  services.tether = {
    enable = true;
    settings.port = 5175;
    android.enable = true; # emulators
  };
}
```

The setup code is in `journalctl --user -u tetherd`.

iPhones need a udev rule, which home-manager cannot install. On NixOS, import the NixOS module too
and set `hardware.tether.enable = true`. On another distribution, copy
[nix/70-tether.rules](nix/70-tether.rules) to `/etc/udev/rules.d`.

### Options

| Option                           | What it does                                                         |
| -------------------------------- | -------------------------------------------------------------------- |
| `services.tether.enable`         | Runs tetherd.                                                        |
| `services.tether.settings`       | tetherd's settings. `port` is required. `tetherd --help` lists them. |
| `services.tether.android.enable` | Turns emulators on, with an Android SDK. They need KVM on Linux.     |
| `services.tether.android.sdk`    | The SDK to use, for other system images than the default one.        |
| `services.tether.gpu.enable`     | NixOS only. A display with no screen, so emulators can use the GPU.  |
| `services.tether.package`        | The tetherd to run.                                                  |
| `hardware.tether.enable`         | NixOS only. The iPhone udev rule, for a tetherd a user runs.         |

`inputs.tether.overlays.default` adds `pkgs.tetherd`, for running it some other way.

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

## AI in development

tether was partly developed using AI. While tether was my first project where I decieded to lean
more heavily into AI coding agents. I had never "vibe-coded" before and I have a lot to say about
the quaility of their output. Blog post soon (tm).

## Licence

MIT or Apache-2.0, at your option.
