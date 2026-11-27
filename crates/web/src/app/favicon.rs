use leptos::prelude::*;

use super::Global;
use crate::link::Linked;

pub(super) fn follow() {
    let Global { link, ui, .. } = expect_context::<Global>();

    Effect::new(move |_| {
        // Read again after a theme change, since the colours come from the
        // computed style.
        ui.theme.track();

        let colour = if link.linked.with(Linked::up) {
            "--ok"
        } else {
            "--bad"
        };

        let Some(root) = document().document_element() else {
            return;
        };

        let Ok(Some(style)) = window().get_computed_style(&root) else {
            return;
        };

        let colour = style.get_property_value(colour).unwrap_or_default();
        let svg = format!(
            "<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 16 16'><rect x='3' y='3' width='10' height='10' fill='{}'/></svg>",
            colour.trim()
        );

        let href = format!("data:image/svg+xml,{}", js_sys::encode_uri_component(&svg));

        if let Ok(Some(icon)) = document().query_selector("link[rel=icon]") {
            let _ = icon.set_attribute("href", &href);
        }
    });
}
