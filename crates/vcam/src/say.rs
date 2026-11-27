//! The log is sent to tetherd, which writes it into the emulator's log,
//! because the emulator drops its own stderr. Lines logged before the
//! connection opens are kept, up to a limit.

use std::os::fd::OwnedFd;
use std::sync::Mutex;

use rustix::net::SendFlags;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::{Context, SubscriberExt};
use vcam_wire::{Level, ToTetherd};

struct Sink;

struct Said {
    to: Option<OwnedFd>,
    kept: Vec<ToTetherd>,
}

static SAID: Mutex<Said> = Mutex::new(Said {
    to: None,
    kept: Vec::new(),
});

impl<S: tracing::Subscriber> Layer<S> for Sink {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let mut message = String::new();

        event.record(&mut Text(&mut message));

        let level = match *event.metadata().level() {
            tracing::Level::ERROR => Level::Error,
            tracing::Level::WARN => Level::Warn,
            tracing::Level::INFO => Level::Info,
            tracing::Level::DEBUG => Level::Debug,
            tracing::Level::TRACE => Level::Trace,
        };

        let said = ToTetherd::Log { level, message };
        let mut sink = SAID.lock().unwrap();

        match &sink.to {
            Some(to) => {
                let flags = SendFlags::NOSIGNAL | SendFlags::DONTWAIT;
                let _ = rustix::net::send(to, &said.encode(), flags);
            }
            None if sink.kept.len() < 64 => sink.kept.push(said),
            None => {}
        }
    }
}

struct Text<'a>(&'a mut String);

impl tracing::field::Visit for Text<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        use std::fmt::Write;

        let _ = match field.name() {
            "message" => write!(self.0, "{value:?}"),
            name => write!(self.0, " {name}={value:?}"),
        };
    }
}

pub(crate) fn start_saying() {
    let level = std::env::var("VSHIM_LOG")
        .ok()
        .and_then(|level| level.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::INFO);

    let _ = tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(Sink.with_filter(level)),
    );
}

pub(crate) fn say_to(connection: OwnedFd) {
    let mut sink = SAID.lock().unwrap();
    let flags = SendFlags::NOSIGNAL | SendFlags::DONTWAIT;

    for said in sink.kept.drain(..) {
        let _ = rustix::net::send(&connection, &said.encode(), flags);
    }

    sink.to = Some(connection);
}
