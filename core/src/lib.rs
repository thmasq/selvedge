#![recursion_limit = "256"]

mod actor;

pub use actor::MatrixWorker;

use gloo_worker::Registrable;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start_worker() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    MatrixWorker::registrar().register();
}
