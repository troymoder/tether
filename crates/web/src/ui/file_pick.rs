//! The browser's own file input stays hidden, and any button of the view
//! opens its chooser through a [`FilePicker`].

use leptos::html;
use leptos::prelude::*;

use crate::util::chosen;

#[derive(Clone, Copy)]
pub(crate) struct FilePicker(NodeRef<html::Input>);

impl FilePicker {
    pub(crate) fn new() -> Self {
        FilePicker(NodeRef::new())
    }

    /// Has to be called from a press or a key, or the browser refuses.
    pub(crate) fn open(self) {
        if let Some(input) = self.0.get_untracked() {
            input.click();
        }
    }
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn FilePick(
    picker: FilePicker,
    #[prop(optional)] accept: Option<&'static str>,
    #[prop(optional)] multiple: bool,
    label: &'static str,
    #[prop(into)] on_file: Callback<web_sys::File>,
) -> impl IntoView {
    let input = picker.0;

    view! {
        <input
            node_ref=input
            hidden
            type="file"
            accept=accept
            multiple=multiple
            aria-label=label
            on:change=move |_| chosen(input, |file| on_file.run(file))
        />
    }
}
