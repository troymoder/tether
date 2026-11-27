// The emulators, the shim and the Unix sockets tetherd uses exist on Linux
// and macOS only, and Cargo.toml declares its dependencies for those alone.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
compile_error!("tetherd builds for Linux and macOS only");

use workspace_hack as _;

mod adb;
mod answer;
mod app;
mod apps;
mod boot;
mod camera;
mod client;
mod db;
mod decode;
mod emulator;
mod frame;
mod http;
mod icon;
mod iphone;
mod journal;
mod known;
mod location;
mod media;
mod name;
mod relay;
mod remote;
mod rtc;
mod settings;
mod task;
mod topics;
mod trust;
mod vcam;
mod watch;
mod webcam;

use std::sync::Arc;

use anyhow::{Context, Result};
use listenfd::ListenFd;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};

use crate::app::App;
use crate::settings::{Command, Flags, Settings};

#[tokio::main]
async fn main() -> Result<()> {
    let flags = <Flags as clap::Parser>::parse();

    if let Some(Command::Config) = flags.command {
        print!("{}", settings::listed(&flags)?);

        return Ok(());
    }

    let mut settings = Settings::read(&flags)?;
    // str0m warns about the data it drops as channels close, which is normal
    // here. Only its errors are shown unless the filter names `str0m`.
    let filter = format!("str0m=error,{}", settings.log.as_deref().unwrap_or("info"));

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_new(&filter)
                .with_context(|| format!("invalid log filter: {filter}"))?,
        )
        .init();

    let port = settings.port;
    let data = settings::data_folder(settings.data.take())?;
    // Bound first, so a second tetherd on a port in use stops here before it
    // takes the first one's camera socket (vcam.rs).
    let listener = listen(port).await?;
    let web = settings.web.take();
    let app =
        Arc::new(App::new(settings, &data, flags.setup, listener.local_addr()?.port()).await?);

    watch::start(&app);

    let router = http::router(&app, web)?;

    match app.origins.first() {
        Some(origin) => tracing::info!("tetherd: http://{origin} (the page), port {port} (this)"),
        None => tracing::info!("tetherd: http://localhost:{port}"),
    }

    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;

    tokio::select! {
        result = http::serve(listener, router, app.hub.clone()) => result?,
        _ = terminate.recv() => {}
        _ = interrupt.recv() => {}
    }

    app.emulators.wait_for_all().await;

    Ok(())
}

/// Whoever reaches the socket controls the devices, so it listens on localhost
/// only. In the dev stack systemfd holds the socket and hands it over.
async fn listen(port: u16) -> Result<TcpListener> {
    Ok(match ListenFd::from_env().take_tcp_listener(0)? {
        Some(listener) => {
            listener.set_nonblocking(true)?;
            TcpListener::from_std(listener)?
        }
        None => TcpListener::bind(("127.0.0.1", port)).await?,
    })
}
