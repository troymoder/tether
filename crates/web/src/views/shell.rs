//! The terminal is xterm.js as glue/terminal.ts wraps it. It lives on its own
//! element, which this view adopts while it is open, so the shell survives
//! switching views.

use leptos::html;
use leptos::prelude::*;

use crate::app::{Global, Picked, icons};
use crate::ui;
use crate::ui::button::{Button, IconButton};
use crate::ui::find::{Find, Finding};
use crate::ui::observe::use_resize;
use crate::ui::pane::Pane;
use crate::ui::tag::{Dot, Tag};
use crate::ui::xterm::{self, colours};

mod console;
mod device;

pub(crate) use console::Console;
use console::{Counted, State, Told};
pub(crate) use device::{Ask, Output, Shell};

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "shell.module.scss"
);

#[derive(Clone, Debug, PartialEq)]
enum Terminal {
    Starting,
    Ready,
    Unable(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Search {
    missed: bool,
    counted: Option<Counted>,
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Shell() -> impl IntoView {
    let ui = expect_context::<Global>().ui;
    let Picked {
        device, console, ..
    } = expect_context::<Picked>();
    let frame = NodeRef::<html::Div>::new();
    let terminal = RwSignal::new(Terminal::Starting);
    // The find bar's text, which is kept while the bar is closed.
    let sought = RwSignal::new(String::new());
    let finding = RwSignal::new(Finding::<Search>::Closed);

    console.report_to(Callback::new(move |told: Told| match told {
        Told::FindPressed => finding.update(Finding::open),
        Told::Counted(now) => {
            // The terminal may report one more count after the text is
            // cleared, which is ignored.
            if sought.with_untracked(String::is_empty) {
                return;
            }

            finding.update(|finding| {
                if let Finding::Open(search) = finding {
                    search.get_or_insert_default().counted = Some(now);
                }
            });
        }
    }));

    let count = move || {
        finding
            .get()
            .outcome()?
            .counted
            .map(|counted| match counted {
                Counted { of: 0, .. } => "None".to_owned(),
                Counted { at: None, of } => of.to_string(),
                Counted { at: Some(at), of } => format!("{} of {of}", at + 1),
            })
    };
    let missed = move || finding.get().outcome().is_some_and(|search| search.missed);
    // Only the find bar calls this, so the bar is open.
    let find = move |backwards: bool| {
        let sought = sought.get_untracked();

        if sought.is_empty() {
            console.with_terminal(xterm::Terminal::end_find);
            finding.set(Finding::Open(None));
        } else {
            console.with_terminal(|made| {
                let missed = !made.find(&sought, backwards);

                finding.update(|finding| {
                    if let Finding::Open(search) = finding {
                        search.get_or_insert_default().missed = missed;
                    }
                });
            });
        }
    };
    let end_find = move || {
        finding.set(Finding::Closed);
        console.with_terminal(|made| {
            made.end_find();
            made.focus();
        });
    };

    frame.on_load(move |frame| {
        if let Err(said) = console.start() {
            terminal.set(Terminal::Unable(said));
            return;
        }

        console.with_terminal(|made| {
            let _ = frame.append_child(&made.host());

            made.colour(&colours());
            made.fit();
            made.focus();
        });

        // A shell that was opened before is kept until Restart.
        if console.state.get_untracked() == State::NotOpened {
            console.open(device.get_untracked());
        }

        terminal.set(Terminal::Ready);
    });

    use_resize(frame, move || console.with_terminal(xterm::Terminal::fit));

    Effect::new(move |_| {
        ui.theme.track();

        if terminal.with(|terminal| *terminal == Terminal::Ready) {
            console.with_terminal(|made| made.colour(&colours()));
        }
    });

    let ended = move || console.state.get() == State::Ended;
    // Only a tag shows here, since the terminal itself shows the reason.
    let noted = move || {
        view! {
            <Show when=ended>
                <Tag tone=Dot::Warn attr:role="status">
                    "Ended"
                </Tag>
            </Show>
        }
    };
    let tools = move || {
        view! {
            <div class=style::actions>
                <Show when=ended>
                    <Button on:click=move |_| {
                        console.open(device.get_untracked());
                        console.with_terminal(xterm::Terminal::focus);
                    }>"Restart"</Button>
                </Show>
                <IconButton
                    paths=icons::SEARCH
                    label="Find · Ctrl+F"
                    framed=true
                    attr:aria-pressed=move || finding.with(Finding::is_open).to_string()
                    on:click=move |_| {
                        if finding.with_untracked(Finding::is_open) {
                            end_find();
                        } else {
                            finding.set(Finding::Open(None));
                        }
                    }
                />
                <Button on:click=move |_| {
                    console
                        .with_terminal(|made| {
                            made.clear();
                            made.focus();
                        });
                }>"Clear"</Button>
            </div>
        }
    };

    view! {
        <Pane title="Shell" class=style::view noted tools>
            <div class=ui::style::pane_body>
                <div node_ref=frame class=style::shell>
                    <Show when=move || finding.with(Finding::is_open)>
                        <Find
                            label="Find in the terminal"
                            sought
                            count=Signal::derive(count)
                            none=Signal::derive(missed)
                            as_typed=true
                            find=move |backwards| find(backwards)
                            close=move |()| end_find()
                        />
                    </Show>
                    {move || match terminal.get() {
                        Terminal::Unable(said) => Some(view! { <p class=style::said>{said}</p> }),
                        Terminal::Starting | Terminal::Ready => None,
                    }}
                </div>
            </div>
        </Pane>
    }
}
