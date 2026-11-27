# The NixOS module runs one tetherd for the machine, as a system service. The
# home-manager module runs one for its user, which is the one that can draw
# emulators through that user's desktop.
packagesFor: let
  options = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.services.tether;
    packages = packagesFor pkgs;
  in {
    config.services.tether.settings.android_sdk = lib.mkIf cfg.android.enable "${cfg.android.sdk}";

    options.services.tether = {
      enable = lib.mkEnableOption "tetherd, which serves tether's page and the devices it shows";

      package = lib.mkOption {
        type = lib.types.package;
        default = packages.default;
        defaultText = lib.literalMD "tetherd from the tether flake, built with this `pkgs`";
        description = "The tetherd to run.";
      };

      android = {
        enable = lib.mkEnableOption "Android emulators, which tetherd runs from an Android SDK";

        sdk = lib.mkOption {
          type = lib.types.package;
          default = packages.android-sdk;
          defaultText = lib.literalMD "an SDK with the emulator and one system image for this machine";
          description = ''
            The SDK the emulators come from: an `androidsdk` from nixpkgs'
            `androidenv` that has the emulator and the system images to offer.
          '';
        };
      };

      settings = lib.mkOption {
        type = lib.types.submodule {
          freeformType = (pkgs.formats.toml {}).type;
          options =
            {
              port = lib.mkOption {
                type = lib.types.port;
                description = "The port tetherd serves on.";
              };

              web = lib.mkOption {
                type = lib.types.path;
                default = "${packages.web}";
                defaultText = lib.literalMD "the page from the tether flake";
                description = "The folder with the built page.";
              };

              android_helper = lib.mkOption {
                type = lib.types.path;
                default = "${packages.helper}";
                defaultText = lib.literalMD "the helper from the tether flake";
                description = "The helper tetherd gives Android devices, as a .dex file.";
              };
            }
            # The shim stands in for Linux's camera devices.
            // lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
              vcam = lib.mkOption {
                type = lib.types.path;
                default = "${packages.vcam}/lib/libvcam.so";
                defaultText = lib.literalMD "the shim from the tether flake";
                description = "The shim that gives emulators their cameras, `libvcam.so`.";
              };
            };
        };
        example = {port = 5175;};
        description = "tetherd's settings, over the package's own. `tetherd --help` lists them.";
      };
    };
  };

  start = pkgs: cfg: "${pkgs.lib.getExe cfg.package} --config ${
    (pkgs.formats.toml {}).generate "tetherd.toml" cfg.settings
  }";

  rules = pkgs: name: text:
    pkgs.writeTextFile {
      name = "tether-udev-rules";
      destination = "/etc/udev/rules.d/${name}";
      inherit text;
    };

  # A system service has no login session to be granted phones through, so its
  # group is. The second line matches adb and fastboot the way systemd's
  # 70-uaccess.rules does.
  serviceRules = ''
    SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="05ac", ATTR{idProduct}=="12[9a][0-9a-f]", GROUP="tether", MODE="0660"
    SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ENV{ID_USB_INTERFACES}=="*:dc0201:*|*:ff4201:*|*:ff4203:*", GROUP="tether", MODE="0660"
  '';
in {
  nixos = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.services.tether;
  in {
    imports = [options];

    # For a user who runs tetherd themselves, from home-manager or by hand.
    options.hardware.tether.enable = lib.mkEnableOption "USB access to iPhones for the logged-in user's tetherd";

    options.services.tether.gpu.enable = lib.mkEnableOption "a display for tetherd's emulators to draw on the GPU through";

    config = lib.mkMerge [
      (lib.mkIf config.hardware.tether.enable {
        services.udev.packages = [(rules pkgs "70-tether.rules" (builtins.readFile ./70-tether.rules))];
      })
      (lib.mkIf cfg.enable {
        services.tether.settings.data = lib.mkDefault "/var/lib/tetherd";

        users.groups.tether = {};
        users.users.tether = {
          isSystemUser = true;
          group = "tether";
          home = "/var/lib/tetherd";
          # The webcams tetherd can feed an emulator's camera from.
          extraGroups = ["video"];
        };

        services.udev.packages = [(rules pkgs "70-tether-service.rules" serviceRules)];

        # tetherd keeps its camera sockets in the user's runtime folder, which
        # a system service does not have. Neither unit owns this one, because
        # systemd removes a unit's own runtime folder when the unit stops, and
        # the other still has sockets in it.
        systemd.tmpfiles.rules = ["d /run/tetherd 0700 tether tether -"];

        systemd.services.tetherd = {
          description = "tetherd";
          wantedBy = ["multi-user.target"];
          environment.XDG_RUNTIME_DIR = "/run/tetherd";
          serviceConfig = {
            ExecStart = start pkgs cfg;
            User = "tether";
            Group = "tether";
            StateDirectory = "tetherd";
            StateDirectoryMode = "0700";
            Restart = "on-failure";
          };
        };
      })
      (lib.mkIf (cfg.enable && cfg.gpu.enable) {
        # An emulator reaches the GPU through an X server, which tetherd gives
        # it, and that server draws through a Wayland compositor. A system
        # service has no desktop, so this is one with no screen. tetherd finds
        # its socket in the runtime folder when an emulator starts.
        systemd.services.tetherd-display = {
          description = "The display tetherd's emulators draw through";
          wantedBy = ["tetherd.service"];
          # Stops after tetherd, which waits for its emulators. An emulator
          # hangs if its display goes before it has saved its snapshot.
          before = ["tetherd.service"];
          environment = {
            XDG_RUNTIME_DIR = "/run/tetherd";
            WLR_BACKENDS = "headless";
            WLR_LIBINPUT_NO_DEVICES = "1";
          };
          serviceConfig = {
            ExecStart = "${lib.getExe pkgs.sway-unwrapped} --config /dev/null";
            User = "tether";
            Group = "tether";
            Restart = "on-failure";
          };
        };
      })
    ];
  };

  homeManager = {
    config,
    lib,
    pkgs,
    ...
  }: let
    cfg = config.services.tether;
  in {
    imports = [options];

    config = lib.mkIf cfg.enable {
      systemd.user.services.tetherd = lib.mkIf pkgs.stdenv.hostPlatform.isLinux {
        Unit.Description = "tetherd";
        Service = {
          ExecStart = start pkgs cfg;
          Restart = "on-failure";
        };
        Install.WantedBy = ["default.target"];
      };

      launchd.agents.tetherd = lib.mkIf pkgs.stdenv.hostPlatform.isDarwin {
        enable = true;
        config = {
          ProgramArguments = lib.splitString " " (start pkgs cfg);
          RunAtLoad = true;
          KeepAlive = true;
        };
      };
    };
  };
}
