#![allow(clippy::large_enum_variant)]

crate::define_top_level_events! {
    pub mod auth;
    pub mod core;
    pub mod crypto;
    pub mod room;
}
