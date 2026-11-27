use android::data::Store;
use leptos::prelude::*;

use super::pane::QueryPane;
use super::{style, use_at};
use crate::app::Picked;
use crate::ui::load::use_load;
use crate::ui::loadable::Status;

fn opened(stores: &[Store], named: Option<&str>) -> Option<Store> {
    stores
        .iter()
        .find(|store| Some(store.name.as_str()) == named)
        .or(stores.first())
        .cloned()
}

#[allow(unreachable_pub)]
#[component]
pub(super) fn StoreNav(app: String, listing: RwSignal<bool>) -> impl IntoView {
    let device = expect_context::<Picked>().device;
    let at = use_at();
    let app = StoredValue::new(app);
    let sought = RwSignal::new(String::new());
    let stores = use_load(
        || (),
        move |()| {
            let device = device.get_untracked();
            let app = app.get_value();

            async move {
                device.stores(&app).await.map_err(|error| {
                    format!(
                        "{error}. What an app keeps is only read on a device that gives root, \
                         like an emulator, or when the app was built to be debugged."
                    )
                })
            }
        },
    );

    view! {
        <Status of=stores empty=(Vec::is_empty, "No databases or preferences.") let:kept>
            <div class=style::inspector>
                <div>
                    {
                        let kept = StoredValue::new(kept);
                        // It changes with the store alone, so a press on a
                        // table builds nothing again.
                        let open = Memo::new(move |_| {
                            let named = at.with(|at| at.store.clone());

                            kept.with_value(|kept| opened(kept, named.as_deref()))
                        });

                        // Built again for each store, so the tables, the
                        // query and its answer are that store's alone.
                        move || {
                            open.get()
                                .map(|store| {
                                    view! {
                                        <QueryPane
                                            app=app.get_value()
                                            store
                                            stores=kept.get_value()
                                            listing
                                            sought
                                        />
                                    }
                                })
                        }
                    }
                </div>
            </div>
        </Status>
    }
}

#[cfg(test)]
mod tests {
    use android::data::Kind;

    use super::*;

    fn store(name: &str) -> Store {
        Store {
            kind: Kind::Database,
            name: name.to_owned(),
        }
    }

    #[test]
    fn the_named_store_is_open_and_otherwise_the_first() {
        let stores = [store("a.db"), store("b.db")];

        assert_eq!(opened(&stores, Some("b.db")), Some(store("b.db")));
        assert_eq!(opened(&stores, None), Some(store("a.db")));
        // A store the address names that the app no longer has.
        assert_eq!(opened(&stores, Some("gone.db")), Some(store("a.db")));
        assert_eq!(opened(&[], Some("a.db")), None);
    }
}
