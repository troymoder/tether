use std::rc::Rc;

use leptos::prelude::*;
use protocol::ItemKind;

use super::fed::Cam;
use super::style;
use crate::app::{Global, Picked};
use crate::link::capture::{self, Remote, Remoting};
use crate::ui::load::use_load;
use crate::ui::loadable::{Loadable, Status};
use crate::ui::pathbar::{self, PathBar};
use crate::util;
use crate::views::files::Typed;

const HOME: &str = "~";

#[derive(Clone, Debug, PartialEq)]
struct Folder {
    /// Its whole path.
    path: String,
    items: Vec<protocol::Item>,
}

/// The browser's place. The source box holds it, so the browser opens
/// where it was after another source was looked at.
#[derive(Clone, Copy)]
pub(super) struct Browsing {
    /// None until it is taken to a folder, when it is at the folder it starts
    /// in.
    at: RwSignal<Option<String>>,
    typed: Typed,
}

impl Browsing {
    pub(super) fn new() -> Self {
        Browsing {
            at: RwSignal::new(None),
            typed: Typed::new(),
        }
    }
}

fn sign(name: &str, folder: bool) -> &'static str {
    let picture = ["jpg", "jpeg", "png", "webp", "gif", "bmp"]
        .iter()
        .any(|end| name.to_lowercase().ends_with(&format!(".{end}")));

    if folder {
        "M3 6h6l2 2h10v11H3z"
    } else if picture {
        "M3 5h18v14H3zM3 16l5-5 4 4 3-3 6 6M15 9.5a1.5 1.5 0 1 0 0-.01"
    } else {
        "M3 6h13v12H3zM16 10l5-3v10l-5-3"
    }
}

struct Row {
    /// Its path from the folder the browser is at.
    path: String,
    name: String,
    about: Option<String>,
    folder: bool,
}

enum Listing {
    Said { what: &'static str, bad: bool },
    Rows { here: String, rows: Vec<Row> },
}

impl Listing {
    fn said(what: &'static str) -> Self {
        Listing::Said { what, bad: false }
    }

    const MISSED: Listing = Listing::Said {
        what: "No such folder or file.",
        bad: true,
    };

    fn found(here: String, matches: Vec<protocol::Match>) -> Self {
        if matches.is_empty() {
            return Listing::said("No match. Enter goes to a path.");
        }

        let rows = matches
            .into_iter()
            .map(|found| {
                let (inside, name) = match found.path.rsplit_once('/') {
                    Some((inside, name)) => (Some(format!("{inside}/")), name.to_owned()),
                    None => (None, found.path.clone()),
                };

                Row {
                    path: found.path,
                    name,
                    about: inside,
                    folder: found.folder,
                }
            })
            .collect();

        Listing::Rows { here, rows }
    }

    fn inside(folder: Folder) -> Self {
        if folder.items.is_empty() {
            return Listing::said("No videos or pictures here.");
        }

        let rows = folder
            .items
            .into_iter()
            .map(|item| {
                let about = match item.kind {
                    ItemKind::Folder { items: 1 } => "1 item".to_owned(),
                    ItemKind::Folder { items } => format!("{items} items"),
                    ItemKind::File { size } => util::size(size),
                };

                Row {
                    path: item.name.clone(),
                    name: item.name,
                    about: Some(about),
                    folder: matches!(item.kind, ItemKind::Folder { .. }),
                }
            })
            .collect();

        Listing::Rows {
            here: folder.path,
            rows,
        }
    }
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn RemoteFiles(cam: Cam, browsing: Browsing, open: RwSignal<bool>) -> impl IntoView {
    let link = expect_context::<Global>().link;
    let feed = expect_context::<Picked>().feed;
    let Browsing { at, typed } = browsing;
    let picked = Memo::new(move |_| match feed.remoting().map(|remote| remote.what) {
        Some(Remote::File { path }) => Some(path),
        _ => None,
    });
    let start = picked
        .get_untracked()
        .and_then(|path| {
            path.rsplit_once('/')
                .map(|(folder, _)| format!("{folder}/"))
        })
        .unwrap_or_else(|| HOME.to_owned());
    let listed = use_load(
        move || at.get().unwrap_or_else(|| start.clone()),
        move |path: String| async move {
            match capture::look(link, path).await? {
                protocol::Looked::Folder { path, items } => Ok(Folder { path, items }),
                protocol::Looked::File { path } => {
                    Err(format!("look failed: not a folder: {path}"))
                }
            }
        },
    );
    let here = Signal::derive(move || {
        listed
            .with(|listed| match listed {
                Loadable::Ready(folder) => Some(folder.path.clone()),
                Loadable::Loading | Loadable::Failed(_) => None,
            })
            .or_else(|| at.get())
    });
    let go = move |path: String| {
        typed.clear();
        at.set(Some(path));
    };
    let on_path = typed.use_path(move |path: String| {
        let path = match here.get_untracked() {
            Some(here) if !path.starts_with('/') && !path.starts_with('~') => {
                format!("{}/{path}", here.trim_end_matches('/'))
            }
            _ => path,
        };

        async move {
            match capture::look(link, path).await {
                Ok(protocol::Looked::Folder { path, .. }) => {
                    // The box may have closed by now.
                    typed.clear();
                    at.try_set(Some(path));

                    true
                }
                Ok(protocol::Looked::File { path }) => {
                    open.try_set(false);

                    let wanted = Remoting {
                        lens: cam.lens,
                        what: Remote::File { path },
                    };

                    feed.start_remote(wanted);

                    true
                }
                Err(_) => false,
            }
        }
    });
    let found = typed.use_found(move |wanted: String| {
        let under = here.get_untracked();

        async move {
            match under {
                Some(under) => capture::find(link, under, wanted).await,
                None => Err("find failed: no folder".to_owned()),
            }
        }
    });
    // While more is typed the last matches stay, so the rows do not blink.
    let matches =
        Memo::new(
            move |last: Option<&Option<Vec<protocol::Match>>>| match found.get()? {
                Loadable::Ready(matches) => Some(matches),
                Loadable::Failed(_) => Some(Vec::new()),
                Loadable::Loading => last.cloned().flatten(),
            },
        );
    let rows = move |folder: Folder| {
        let listing = if typed.missing() {
            Listing::MISSED
        } else {
            match matches.get() {
                Some(matches) => Listing::found(folder.path, matches),
                None => Listing::inside(folder),
            }
        };

        match listing {
            Listing::Said { what, bad } => said(what.to_owned(), bad),
            Listing::Rows { here, rows } => {
                let picked = picked.get();

                rows.into_iter()
                    .map(|row| {
                        let to = format!("{}/{}", here.trim_end_matches('/'), row.path);
                        let on = picked.as_deref() == Some(to.as_str());
                        let kind = sign(&row.name, row.folder);
                        let is_folder = row.folder;
                        let name = if is_folder {
                            format!("{}/", row.name)
                        } else {
                            row.name
                        };

                        view! {
                            <button
                                type="button"
                                role="option"
                                aria-selected=on.to_string()
                                class=style::file
                                on:click=move |_| {
                                    if is_folder {
                                        go(to.clone());
                                    } else {
                                        on_path.run(to.clone());
                                    }
                                }
                            >
                                <svg aria-hidden="true" viewBox="0 0 24 24">
                                    <path d=kind />
                                </svg>
                                <span>{name}</span>
                                <code>{row.about}</code>
                            </button>
                        }
                    })
                    .collect_view()
                    .into_any()
            }
        }
    };
    let waiting = move |what: String| {
        let failed = listed.with_untracked(|listed| matches!(listed, Loadable::Failed(_)));

        said(what, failed)
    };
    let folders: pathbar::Folders = Rc::new(move |of: String| {
        Box::pin(async move {
            match capture::look(link, of).await? {
                protocol::Looked::Folder { path, items } => Ok(items
                    .into_iter()
                    .filter(|item| matches!(item.kind, ItemKind::Folder { .. }))
                    .map(|item| {
                        let to = format!("{}/{}", path.trim_end_matches('/'), item.name);

                        (item.name, to)
                    })
                    .collect()),
                // Every part of the bar's path is a folder.
                protocol::Looked::File { .. } => Ok(Vec::new()),
            }
        })
    });
    let path = Signal::derive(move || here.get().unwrap_or_default());

    view! {
        <PathBar path folders on_go=go find=typed.text on_path />
        <div class=style::files role="listbox" aria-label="Files">
            <Status of=listed line=waiting let:folder>
                {move || rows(folder.clone())}
            </Status>
        </div>
    }
}

fn said(what: String, bad: bool) -> AnyView {
    view! {
        <span class=style::said class=(style::bad, bad)>
            {what}
        </span>
    }
    .into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_is_signed_by_its_kind() {
        assert_eq!(sign("Holiday", true), "M3 6h6l2 2h10v11H3z");
        assert_eq!(sign("beach.JPG", false), sign("a.png", false));
        assert_ne!(sign("clip.mp4", false), sign("a.png", false));
    }

    #[test]
    fn a_match_is_listed_with_the_folder_it_is_in() {
        let matches = vec![
            protocol::Match {
                path: "trips/beach.mp4".to_owned(),
                folder: false,
            },
            protocol::Match {
                path: "top.png".to_owned(),
                folder: false,
            },
        ];

        let Listing::Rows { here, rows } = Listing::found("/home/me".to_owned(), matches) else {
            panic!("matches are rows");
        };

        assert_eq!(here, "/home/me");
        assert_eq!(rows[0].name, "beach.mp4");
        assert_eq!(rows[0].about.as_deref(), Some("trips/"));
        assert_eq!(rows[1].about, None);
        assert!(matches!(
            Listing::found("/home/me".to_owned(), Vec::new()),
            Listing::Said { bad: false, .. }
        ));
    }
}
