use leptos::prelude::*;
use protocol::App;

use super::{Asked, of};
use crate::app::Global;
use crate::ui::menu::{Item, MenuOf, Row};

#[allow(unreachable_pub)]
#[component]
pub(super) fn InstallMenu(menu: MenuOf<u32>, app: Memo<App>, asked: Asked) -> impl IntoView {
    let link = expect_context::<Global>().link;

    menu.view(false, move |build| {
        let build = *build;
        let app = app.get();
        let mut items: Vec<Item> = asked
            .get()
            .into_iter()
            .filter_map(|(device, _)| {
                let listed = link.device(&device)?;
                let goes = listed
                    .described()
                    .and_then(|described| app.variant(&described.abis))
                    .map(|variant| variant.build);
                let app = app.clone();

                Some(Item::Row(Row {
                    what: listed.name.clone(),
                    icon: None,
                    does_icon: None,
                    place: None,
                    ticked: false,
                    does: Some("Install".to_owned()),
                    undoes: false,
                    on: Callback::new(move |()| {
                        // A device can only be given the build its ABIs pick.
                        if goes != Some(build) {
                            link.unimplemented("install another build");

                            return;
                        }

                        // The device may have disconnected since the menu opened.
                        let installed = asked.with_untracked(|asked| of(asked, &device));

                        if let Some(installed) = installed {
                            installed.install(&app);
                        }
                    }),
                }))
            })
            .collect();

        if items.is_empty() {
            items.push(Item::Label("No device for it is connected."));
        }

        Some((format!("Install build {build} on"), items))
    })
}
