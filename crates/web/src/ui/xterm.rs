//! xterm.js as glue/terminal.ts wraps it.

use wasm_bindgen::prelude::*;
use web_sys::HtmlElement;

mod colours;

pub(crate) use colours::{colours, token_number};

#[wasm_bindgen]
extern "C" {
    /// A terminal of either kind. Methods marked for the shell or for a log
    /// only work on that kind.
    pub type Terminal;

    /// `sized` gets the columns and rows, and `counted` the current match and
    /// the total. The index is -1 when there are too many to count.
    #[wasm_bindgen(catch, js_namespace = tetherTerminal)]
    pub fn shell(
        typed: &Closure<dyn Fn(String)>,
        sized: &Closure<dyn Fn(u16, u16)>,
        find: &Closure<dyn Fn()>,
        counted: &Closure<dyn Fn(i32, u32)>,
    ) -> Result<Terminal, JsValue>;

    #[wasm_bindgen(catch, js_namespace = tetherTerminal)]
    pub fn log() -> Result<Terminal, JsValue>;

    #[wasm_bindgen(method, getter)]
    pub fn host(this: &Terminal) -> HtmlElement;

    #[wasm_bindgen(method)]
    pub fn fit(this: &Terminal);

    #[wasm_bindgen(method)]
    pub fn focus(this: &Terminal);

    #[wasm_bindgen(method)]
    pub fn columns(this: &Terminal) -> u16;

    #[wasm_bindgen(method)]
    pub fn rows(this: &Terminal) -> u16;

    /// In CSS pixels.
    #[wasm_bindgen(method, js_name = rowHeight)]
    pub fn row_height(this: &Terminal) -> f64;

    #[wasm_bindgen(method)]
    pub fn colour(this: &Terminal, theme: &JsValue);

    /// Shell only.
    #[wasm_bindgen(method)]
    pub fn write(this: &Terminal, printed: &[u8]);

    /// Shell only.
    #[wasm_bindgen(method)]
    pub fn reset(this: &Terminal);

    /// Shell only.
    #[wasm_bindgen(method)]
    pub fn clear(this: &Terminal);

    /// Shell only. Highlights every match, and returns true if there is any.
    #[wasm_bindgen(method)]
    pub fn find(this: &Terminal, text: &str, backwards: bool) -> bool;

    /// Shell only.
    #[wasm_bindgen(method, js_name = endFind)]
    pub fn end_find(this: &Terminal);

    /// Log only.
    #[wasm_bindgen(method)]
    pub fn draw(this: &Terminal, lines: &str);
}
