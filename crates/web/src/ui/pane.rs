//! The content that scrolls goes in a `<div class=ui::style::pane_body>`.
//! Handlers and attributes given to the pane go on its `<section>`.

use leptos::prelude::*;

use crate::ui;

stylance::import_style!(
    #[allow(unreachable_pub)]
    style,
    "pane.module.scss"
);

#[derive(Clone, Copy)]
pub(crate) struct Crumb {
    pub name: &'static str,
    pub back: Callback<()>,
}

#[allow(unreachable_pub)]
#[component]
pub(crate) fn Pane(
    #[prop(into)] title: Signal<String>,
    #[prop(optional)] crumbs: Option<Crumb>,
    #[prop(optional, into)] noted: Option<ViewFn>,
    #[prop(optional, into)] tools: ViewFn,
    #[prop(optional, into)] under: ViewFn,
    #[prop(optional)] class: &'static str,
    children: Children,
) -> impl IntoView {
    let name = match crumbs {
        Some(Crumb { name, back }) => view! {
            <h1 class=style::crumbs>
                <button type="button" on:click=move |_| back.run(())>
                    {name}
                </button>
                <span aria-hidden="true">"›"</span>
                <span>{title}</span>
            </h1>
        }
        .into_any(),
        None => view! { <h1>{title}</h1> }.into_any(),
    };
    let named = match noted {
        Some(noted) => view! { <span class=style::named>{name}{noted.run()}</span> }.into_any(),
        None => name,
    };

    view! {
        <section class=class>
            <div class=ui::style::pane_top>
                <header class=ui::style::view_head>{named} {tools.run()}</header>
                {under.run()}
            </div>
            {children()}
        </section>
    }
}
