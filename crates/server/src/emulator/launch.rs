use std::process::Stdio;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use protocol::{EmulatorSettings, Graphics, Lens};
use tokio::process::{Child, Command};
use tokio::sync::{Notify, watch};
use tokio::time::{sleep, timeout};

use super::camera::follow_turns;
use super::log::{follow, keep};
use super::sdk::Gpu;
use super::{Emulators, Running, Saved};
use crate::boot;
use crate::camera::Fed;
use crate::vcam;

struct Fitted {
    /// Back, then front.
    fed: [Option<Arc<Fed>>; 2],
    lenses: Vec<vcam::Lens>,
    /// The cameras' formats, which decide if the quick-boot snapshot fits.
    booted_with: String,
}

impl Emulators {
    pub(crate) async fn start(self: &Arc<Self>, name: &str) -> Result<()> {
        self.set_starting(name, true);
        let started = self.run(name).await;

        self.set_starting(name, false);

        started
    }

    async fn run(self: &Arc<Self>, name: &str) -> Result<()> {
        let _busy = self.busy.lock().await;
        let (Saved { port, settings, .. }, running) = self.find(name)?;

        if running.is_some() {
            return Ok(());
        }

        kvm()?;

        let log = self.log_of(name)?;

        log.lock().unwrap().clear();
        self.make_avd(name, &settings, &log).await?;

        let gpu = match settings.graphics {
            Graphics::Gpu => self.gpu(),
            Graphics::Software => None,
        };
        let display = format!(":{}", port % 1000);
        let mut x_server = None;

        if let Some(Gpu::Wayland { satellite, socket }) = &gpu {
            x_server = Some(start_x_server(satellite, socket, &display).await?);
        }

        let mut emulator = self.command(name, port, &settings, gpu.is_some())?;
        let cameras = self.fit_cameras(&mut emulator, name, port, &settings)?;

        self.drop_stale_snapshot(name, &cameras.booted_with).await?;

        if x_server.is_some() {
            emulator.env("DISPLAY", &display);
        }

        let emulator = self.spawn(name, emulator, cameras.lenses, &log)?;

        self.watch(name, emulator, x_server, cameras.fed);

        Ok(())
    }

    fn command(
        &self,
        name: &str,
        port: u16,
        settings: &EmulatorSettings,
        gpu: bool,
    ) -> Result<Command> {
        let mut emulator = self.tool("libexec/android-sdk/emulator/emulator")?;

        emulator
            .args(["-avd", name, "-port", &port.to_string()])
            .args(["-memory", &settings.memory.to_string()])
            // Its own window only draws through X, and freezes a Wayland
            // desktop.
            .args(["-no-window", "-no-audio"])
            // Its gRPC service shows the screen while it boots (boot.rs). With
            // a token it listens on this machine only.
            .args(["-grpc", &boot::port(port).to_string(), "-grpc-use-token"])
            // Without Vulkan, apps in it draw with OpenGL. With Vulkan the
            // browser's drawing hangs every half minute and the screen stands
            // still. The emulator also skips its snapshot when it stops while
            // an app draws with Vulkan, so every start is a cold boot.
            // shortcut: off for all, make it a setting if an app needs it.
            .args(["-feature", "-Vulkan"])
            .args(["-gpu", if gpu { "host" } else { "swiftshader_indirect" }]);

        Ok(emulator)
    }

    /// The shim makes the cameras up as `/dev/video0` and on, which the
    /// emulator names `webcam0` and on in that order. A side without a camera
    /// is set to `none`, or the emulator's own config would decide.
    fn fit_cameras(
        &self,
        emulator: &mut Command,
        name: &str,
        port: u16,
        settings: &EmulatorSettings,
    ) -> Result<Fitted> {
        let mut fed = [None, None];
        let mut lenses = Vec::new();
        let mut booted_with = Vec::new();

        for lens in Lens::ALL {
            let Some(camera) = settings.camera(lens) else {
                emulator.args([format!("-camera-{}", lens.name()), "none".to_owned()]);
                continue;
            };

            let index = lenses.len();
            let format = vcam::Format {
                width: camera.width.into(),
                height: camera.height.into(),
                fps: camera.fps.into(),
                fourcc: vcam::FourCc::Yu12,
                range: vcam::Range::Limited,
            };
            let fed_camera = Arc::new(Fed::start(camera.width.into(), camera.height.into()));

            tokio::spawn(follow_turns(port, Arc::downgrade(&fed_camera)));

            lenses.push(vcam::Lens {
                device: format!("/dev/video{index}"),
                format,
                frames: fed_camera.frames(),
            });

            booted_with.push(format!("{}={format}", lens.name()));
            emulator.args([format!("-camera-{}", lens.name()), format!("webcam{index}")]);
            fed[lens.at()] = Some(fed_camera);
        }

        if !lenses.is_empty() {
            let shim = self
                .shim
                .as_ref()
                .context("camera shim not found: libvcam.so")?;

            emulator
                .env("LD_PRELOAD", shim)
                .env("VSHIM_SOCKET", self.cameras.socket())
                .env("VSHIM_EMULATOR", name);
        }

        Ok(Fitted {
            fed,
            lenses,
            booted_with: booted_with.join(" "),
        })
    }

    /// The cameras are registered before the emulator is spawned so its shim
    /// never asks too early, and until its pid is known any process started
    /// from here passes as it.
    fn spawn(
        &self,
        name: &str,
        mut emulator: Command,
        lenses: Vec<vcam::Lens>,
        log: &Arc<Mutex<String>>,
    ) -> Result<Child> {
        let shim_log: vcam::Log = {
            let log = log.clone();

            Arc::new(move |line: &str| keep(&log, line))
        };

        let early = lenses
            .iter()
            .map(|lens| vcam::Lens {
                device: lens.device.clone(),
                format: lens.format,
                frames: lens.frames.clone(),
            })
            .collect();

        self.cameras
            .add(name, std::process::id(), early, shim_log.clone());

        let spawned = emulator
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut emulator = match spawned {
            Ok(emulator) => emulator,
            Err(error) => {
                self.cameras.remove(name);

                return Err(error.into());
            }
        };

        if let Some(pid) = emulator.id() {
            self.cameras.add(name, pid, lenses, shim_log);
        }

        if let Some(stdout) = emulator.stdout.take() {
            tokio::spawn(follow(stdout, log.clone()));
        }

        if let Some(stderr) = emulator.stderr.take() {
            tokio::spawn(follow(stderr, log.clone()));
        }

        Ok(emulator)
    }

    fn watch(
        self: &Arc<Self>,
        name: &str,
        mut emulator: Child,
        x_server: Option<Child>,
        cameras: [Option<Arc<Fed>>; 2],
    ) {
        let (has_exited, exited) = watch::channel(false);
        let kill = Arc::new(Notify::new());

        self.set_running(
            name,
            Some(Running {
                exited,
                kill: kill.clone(),
                stopping: Arc::default(),
                booted: Arc::default(),
                pid: emulator.id(),
                cameras,
            }),
        );

        let emulators = self.clone();
        let name = name.to_owned();

        tokio::spawn(async move {
            tokio::select! {
                _ = emulator.wait() => {}
                _ = kill.notified() => {
                    let _ = emulator.kill().await;
                }
            }

            // The emulator saves its snapshot through the X server, so the
            // server goes only after it has exited.
            if let Some(mut x_server) = x_server {
                let _ = x_server.kill().await;
            }

            emulators.cameras.remove(&name);
            emulators.set_running(&name, None);
            let _ = has_exited.send(true);
        });
    }

    /// `ask` is false when something else has already told it to stop.
    pub(crate) async fn stop(&self, name: &str, ask: bool) -> Result<()> {
        // A start under way finishes first, or the emulator would boot after
        // this said it had stopped. The lock is released at once so stops do
        // not wait on each other.
        drop(self.busy.lock().await);

        let (saved, Some(mut running)) = self.find(name)? else {
            return Ok(());
        };

        running.stopping.store(true, Ordering::Relaxed);

        if ask {
            // Quits it the way its window's close button would, which saves
            // the snapshot that makes the next start fast. A console that
            // hangs is waited out like an emulator that does not quit.
            let _ = timeout(
                Duration::from_secs(5),
                crate::adb::host::console(saved.port, "kill"),
            )
            .await;
        }

        let exited = running.exited.wait_for(|exited| *exited);

        if timeout(Duration::from_secs(60), exited).await.is_err() {
            running.kill.notify_one();
            let _ = running.exited.wait_for(|exited| *exited).await;
        }

        Ok(())
    }

    /// Each emulator is asked because the signal does not reach one started in
    /// another process group. Waiting keeps their X servers alive until they
    /// have saved their snapshots.
    pub(crate) async fn wait_for_all(self: &Arc<Self>) {
        let names: Vec<String> = {
            let list = self.list.lock().unwrap();

            list.iter().map(|entry| entry.saved.name.clone()).collect()
        };
        let mut stopping = tokio::task::JoinSet::new();

        for name in names {
            let emulators = self.clone();

            stopping.spawn(async move { emulators.stop(&name, true).await });
        }

        stopping.join_all().await;
    }

    pub(crate) fn autostart(self: &Arc<Self>) {
        let names: Vec<String> = {
            let list = self.list.lock().unwrap();

            list.iter()
                .filter(|entry| entry.saved.settings.autostart)
                .map(|entry| entry.saved.name.clone())
                .collect()
        };

        for name in names {
            let emulators = self.clone();

            tokio::spawn(async move {
                if let Err(error) = emulators.start(&name).await {
                    tracing::error!("{name} did not start: {error:#}");
                }
            });
        }
    }
}

/// The emulator does not run an image of this machine's architecture without
/// KVM, and its own message for that is easy to miss in its output.
#[cfg(target_os = "linux")]
fn kvm() -> Result<()> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
        .map(drop)
        .context("Android emulators need KVM, and /dev/kvm cannot be opened")
}

#[cfg(not(target_os = "linux"))]
fn kvm() -> Result<()> {
    Ok(())
}

/// The X server runs in a session of its own so stopping tetherd does not reach
/// it, because the emulator hangs if the server goes before it has saved its
/// snapshot.
async fn start_x_server(
    satellite: &std::path::Path,
    socket: &std::ffi::OsStr,
    display: &str,
) -> Result<Child> {
    let x_server = Command::new("setsid")
        .arg(satellite)
        .arg(display)
        .env("WAYLAND_DISPLAY", socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("setsid")?;

    sleep(Duration::from_secs(1)).await;

    Ok(x_server)
}
