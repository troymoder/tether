use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

use adb::Said;
use futures_channel::mpsc;
use futures_util::StreamExt;
use leptos::prelude::*;
use wasm_bindgen::closure::Closure;

use super::device::Ask;
use crate::device::Device;
use crate::ui::xterm::{self, Terminal};

/// The terminal with the callbacks it calls, kept alive as long as it is.
struct Session {
    terminal: Terminal,
    _typed: Closure<dyn Fn(String)>,
    _sized: Closure<dyn Fn(u16, u16)>,
    _find: Closure<dyn Fn()>,
    _counted: Closure<dyn Fn(i32, u32)>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Counted {
    /// None when there are too many matches for the terminal to place the
    /// current one.
    pub(super) at: Option<u32>,
    pub(super) of: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Told {
    FindPressed,
    Counted(Counted),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum State {
    NotOpened,
    Running,
    Ended,
}

/// It is made when the device is picked, so the shell keeps running while
/// another view is open. The shell is closed when another device is picked.
#[derive(Clone, Copy)]
pub(crate) struct Console {
    kept: StoredValue<Kept, LocalStorage>,
    pub(super) state: RwSignal<State>,
}

#[derive(Default)]
struct Kept {
    session: OnceCell<Session>,
    /// None while the Shell view is closed.
    heard: Cell<Option<Callback<Told>>>,
    /// Dropping the sender ends the shell.
    asks: RefCell<Option<mpsc::UnboundedSender<Ask>>>,
    /// Counts opened shells, so output from a replaced one is ignored.
    opened: Cell<u32>,
}

impl Console {
    pub(crate) fn new() -> Self {
        let console = Self {
            kept: StoredValue::new_local(Kept::default()),
            state: RwSignal::new(State::NotOpened),
        };

        on_cleanup(move || {
            console.with(|kept| kept.asks.take());
        });

        console
    }

    /// None once the console is disposed, which the task of a shell that is
    /// still ending may run into.
    fn with<T>(self, then: impl FnOnce(&Kept) -> T) -> Option<T> {
        self.kept.try_with_value(then)
    }

    fn terminal<T>(self, read: impl FnOnce(&Terminal) -> T) -> Option<T> {
        self.with(|kept| Some(read(&kept.session.get()?.terminal)))?
    }

    fn ask(self, ask: Ask) {
        self.with(|kept| {
            if let Some(asks) = &*kept.asks.borrow() {
                let _ = asks.unbounded_send(ask);
            }
        });
    }

    fn tell(self, told: Told) {
        self.with(|kept| {
            if let Some(heard) = kept.heard.get() {
                heard.run(told);
            }
        });
    }

    pub(super) fn report_to(self, heard: Callback<Told>) {
        self.with(|kept| kept.heard.set(Some(heard)));

        on_cleanup(move || {
            self.with(|kept| kept.heard.set(None));
        });
    }

    pub(super) fn start(self) -> Result<(), String> {
        if self.terminal(|_| ()).is_some() {
            return Ok(());
        }

        let typed = Closure::new(move |sent: String| self.ask(Ask::Typed(sent.into_bytes())));
        let sized =
            Closure::new(move |columns: u16, rows: u16| self.ask(Ask::Size { columns, rows }));
        let find = Closure::new(move || self.tell(Told::FindPressed));
        // The terminal gives -1 for a match it cannot place.
        let counted = Closure::new(move |at: i32, of: u32| {
            self.tell(Told::Counted(Counted {
                at: u32::try_from(at).ok(),
                of,
            }));
        });
        let terminal = xterm::shell(&typed, &sized, &find, &counted)
            .map_err(|_| "shell failed: the terminal did not load".to_owned())?;

        self.with(|kept| {
            let _ = kept.session.set(Session {
                terminal,
                _typed: typed,
                _sized: sized,
                _find: find,
                _counted: counted,
            });
        });

        Ok(())
    }

    /// Writes the reason in warning colour and hides the cursor, since
    /// nothing can be typed into an ended shell.
    fn end(self, said: String) {
        self.with_terminal(|terminal| {
            terminal.write(format!("\r\n\x1b[33m{said}\x1b[0m\x1b[?25l").as_bytes());
        });

        self.state.try_set(State::Ended);
    }

    pub(super) fn open(self, shelled: Rc<dyn Device>) {
        self.with(|kept| kept.asks.take());

        let Some(round) = self.with(|kept| {
            let round = kept.opened.get().wrapping_add(1);

            kept.opened.set(round);

            round
        }) else {
            return;
        };

        let Some(size) = self.terminal(|terminal| {
            terminal.reset();
            terminal.fit();

            (terminal.columns(), terminal.rows())
        }) else {
            return;
        };

        self.state.set(State::Running);

        // The channel exists before the shell opens, so keys typed meanwhile
        // wait in it.
        let (asks, asked) = mpsc::unbounded();

        self.with(|kept| kept.asks.replace(Some(asks)));

        leptos::task::spawn_local(async move {
            let current = move || self.with(|kept| kept.opened.get()) == Some(round);
            let mut output = match shelled.shell(size, asked).await {
                Ok(output) => output,
                Err(error) => {
                    if current() {
                        self.with(|kept| kept.asks.take());
                        self.end(error);
                    }

                    return;
                }
            };

            let said = loop {
                match output.next().await {
                    Some(Ok(Said::Stdout(bytes) | Said::Stderr(bytes))) => {
                        if !current() {
                            return;
                        }

                        self.with_terminal(|terminal| terminal.write(&bytes));
                    }
                    Some(Ok(Said::Exit(0))) | None => break "shell ended".to_owned(),
                    Some(Ok(Said::Exit(code))) => break format!("shell ended: exit code {code}"),
                    Some(Err(error)) => break error,
                }
            };

            if current() {
                self.with(|kept| kept.asks.take());
                self.end(said);
            }
        });
    }

    pub(super) fn with_terminal(self, run: impl FnOnce(&Terminal)) {
        self.terminal(run);
    }
}
