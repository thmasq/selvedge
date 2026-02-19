#![recursion_limit = "256"]

mod actor;
mod model;

pub use actor::{MatrixWorker, ToActor, ToShell};
pub use model::{
    ActiveCallState, CallParticipant, CallType, DeliveryStatus, EventItem, MemberProfile,
    RoomDetails, RoomSummary, TimelineContent, TimelineItem, VirtualItem,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main_js() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}
