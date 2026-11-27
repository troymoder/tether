use leptos::prelude::*;

use super::super::installed::Installed;
use super::Entry;
use crate::app::{Global, Page};
use crate::ui::menu::Item;

pub(in super::super) fn items(installed: Installed, entry: &Entry) -> Vec<Item> {
    let Global { link, router, ui } = expect_context::<Global>();
    let id = entry.package.id.clone();
    let name = entry.name();
    let registered = entry.app.is_some();
    let details = Callback::new(move |()| link.unimplemented("app details"));
    let registry = {
        let id = id.clone();

        Callback::new(move |()| {
            if registered {
                router.go(Page::Registry {
                    app: Some(id.clone()),
                });
            } else {
                installed.hand_over(id.clone());
            }
        })
    };
    let clear = {
        let id = id.clone();
        let name = name.clone();

        Callback::new(move |()| {
            let id = id.clone();
            let name = name.clone();

            ui.confirm_then(
                format!("Clear the data of {name}?"),
                "Everything it keeps on the device goes.",
                "Clear",
                async move { installed.clear(id, &name) },
            );
        })
    };
    let uninstall = Callback::new(move |()| {
        let id = id.clone();
        let name = name.clone();

        ui.confirm_then(
            format!("Uninstall {name}?"),
            "What it has kept on the device goes too.",
            "Uninstall",
            async move { installed.uninstall(id, &name) },
        );
    });
    let mut items = vec![
        Item::Action("Details", details),
        Item::Action(
            if registered {
                "Open in Registry"
            } else {
                "Add to registry"
            },
            registry,
        ),
        Item::Rule,
        Item::Keyed {
            text: "Clear data",
            keys: None,
            red: true,
            on: clear,
        },
    ];

    // A system app came with the device and stays on it.
    if !entry.package.system {
        items.push(Item::Keyed {
            text: "Uninstall",
            keys: None,
            red: true,
            on: uninstall,
        });
    }

    items
}
