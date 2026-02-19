use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main_js() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}
