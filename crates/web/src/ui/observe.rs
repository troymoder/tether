use leptos::html::ElementType;
use leptos::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;
use web_sys::{Element, ResizeObserver};

type Heard = Closure<dyn Fn(js_sys::Array)>;

pub(crate) fn use_resize<E>(node: NodeRef<E>, resized: impl Fn() + 'static)
where
    E: ElementType + 'static,
    E::Output: JsCast + Clone + 'static,
{
    let watching = StoredValue::new_local(None::<(ResizeObserver, Heard)>);

    node.on_load(move |element| {
        let heard: Heard = Closure::new(move |_: js_sys::Array| resized());

        if let Ok(observer) = ResizeObserver::new(heard.as_ref().unchecked_ref()) {
            observer.observe(element.unchecked_ref::<Element>());
            watching.try_set_value(Some((observer, heard)));
        }
    });

    on_cleanup(move || {
        watching.try_with_value(|watching| {
            if let Some((observer, _)) = watching {
                observer.disconnect();
            }
        });
    });
}
