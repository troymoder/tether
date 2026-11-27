//! The list makes one [`Renaming`], which holds the row that is being
//! renamed, and each row shows its name through [`Renamable`].

use leptos::prelude::*;

use super::name_field::NameField;

pub(crate) struct Renaming<K: 'static>(RwSignal<Option<K>>);

impl<K> Clone for Renaming<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for Renaming<K> {}

impl<K> Renaming<K>
where
    K: Clone + PartialEq + Send + Sync + 'static,
{
    pub(crate) fn new() -> Self {
        Renaming(RwSignal::new(None))
    }

    pub(crate) fn start(self, of: K) {
        self.0.set(Some(of));
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Renamable<K>(
    renaming: Renaming<K>,
    of: K,
    #[prop(into)] name: Signal<String>,
    #[prop(into)] renamed: Callback<String>,
    #[prop(optional, into)] done: Option<Callback<()>>,
    /// The class of a `<span>` put around the field, where the field needs
    /// the name's font.
    #[prop(optional)]
    class: Option<&'static str>,
    children: ChildrenFn,
) -> impl IntoView
where
    K: Clone + PartialEq + Send + Sync + 'static,
{
    let mine = Memo::new(move |_| renaming.0.with(|renamed| renamed.as_ref() == Some(&of)));
    let ended = move |()| {
        renaming.0.set(None);

        if let Some(done) = done {
            done.run(());
        }
    };

    move || {
        if !mine.get() {
            return children().into_any();
        }

        let field = view! { <NameField name renamed done=ended /> };

        match class {
            Some(class) => view! { <span class=class>{field}</span> }.into_any(),
            None => field.into_any(),
        }
    }
}
