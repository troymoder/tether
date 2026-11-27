# Boots a machine for each way the modules run tetherd, and checks that it
# serves the page on each.
#
#   nixos   the NixOS module's system service
#   gpu     the same, with emulators and the display they reach the GPU through
#   home    a user's service from the home-manager module alone
#   both    the system service and a user's service side by side, on two ports
{
  pkgs,
  modules,
  home-manager,
}: let
  system = 5175;
  user = 5176;

  # home-manager for alice, whose services start at boot because she lingers.
  alice = port: {
    imports = [home-manager.nixosModules.home-manager];

    users.users.alice = {
      isNormalUser = true;
      uid = 1000;
      linger = true;
    };

    home-manager = {
      # The machine's `pkgs` has the Android SDK's licence accepted.
      useGlobalPkgs = true;
      users.alice = {
        imports = [modules.homeManager];
        home.stateVersion = "25.11";
        services.tether = {
          enable = true;
          settings.port = port;
        };
      };
    };
  };

  service = {
    enable = true;
    settings.port = system;
  };
in
  pkgs.testers.runNixOSTest {
    name = "tether";

    nodes.nixos = {
      imports = [modules.nixos];
      services.tether = service;
    };

    nodes.gpu = {
      imports = [modules.nixos];
      services.tether =
        service
        // {
          android.enable = true;
          gpu.enable = true;
        };
      # tetherd starts the first of these for an emulator, and the second
      # asks the X server it gives what it is.
      environment.systemPackages = [pkgs.xwayland-satellite pkgs.xdpyinfo];
    };

    nodes.home.imports = [(alice user)];

    nodes.both = {
      imports = [modules.nixos (alice user)];
      services.tether = service;
      hardware.tether.enable = true;
    };

    testScript = ''
      def serves(machine, port, journal):
          machine.wait_for_open_port(port)
          machine.succeed(f"curl -sf http://127.0.0.1:{port}/ | grep -q '<title>tether</title>'")

          # The first browser needs the setup code, and the journal is where a user finds it.
          machine.succeed(f"journalctl {journal} | grep -q 'Setup code'")

      def system_service(machine):
          machine.wait_for_unit("tetherd.service")
          serves(machine, ${toString system}, "-u tetherd")
          machine.succeed("test -e /etc/udev/rules.d/70-tether-service.rules")
          machine.succeed("test $(stat -c %U:%a /var/lib/tetherd) = tether:700")

      def user_service(machine):
          machine.wait_for_unit("home-manager-alice.service")
          machine.wait_for_unit("user@1000.service")
          machine.wait_for_unit("tetherd.service", "alice")
          serves(machine, ${toString user}, "_SYSTEMD_USER_UNIT=tetherd.service")

      start_all()

      with subtest("the NixOS module"):
          system_service(nixos)
          nixos.fail("systemctl cat tetherd-display.service")

          settings = "$(systemctl show -P ExecStart tetherd | grep -o '/nix/store/[^ ]*tetherd.toml' | head -1)"
          for name in ["web", "android_helper", "vcam"]:
              nixos.succeed(f"test -e $(grep '^{name} = ' {settings} | cut -d'\"' -f2)")

      with subtest("the NixOS module with emulators and a display"):
          system_service(gpu)
          gpu.wait_for_unit("tetherd-display.service")
          gpu.wait_until_succeeds("test -S /run/tetherd/wayland-1", timeout=60)

          # The machine has no GPU, so this cannot show hardware rendering. It
          # shows that tetherd's user can run an X server on the display.
          as_tetherd = "runuser -u tether -- env XDG_RUNTIME_DIR=/run/tetherd"
          gpu.succeed(f"{as_tetherd} WAYLAND_DISPLAY=wayland-1 xwayland-satellite :7 >&2 &")
          gpu.wait_until_succeeds(f"{as_tetherd} DISPLAY=:7 xdpyinfo | grep -q 'vendor string'", timeout=60)

          # No emulator is started, which would need KVM inside this machine.
          # This shows tetherd is given the SDK and its user can run the tools.
          sdk = gpu.succeed("grep -o '/nix/store/[^\"]*androidsdk' $(systemctl show -P ExecStart tetherd | grep -o '/nix/store/[^ ]*tetherd.toml' | head -1)").strip()
          gpu.succeed(f"{as_tetherd} HOME=/var/lib/tetherd {sdk}/bin/avdmanager list device -c | grep -q pixel_6")

      with subtest("the home-manager module"):
          user_service(home)
          home.fail("systemctl cat tetherd.service")

      with subtest("both modules on one machine"):
          system_service(both)
          user_service(both)
          both.succeed("test -e /etc/udev/rules.d/70-tether.rules")
    '';
  }
