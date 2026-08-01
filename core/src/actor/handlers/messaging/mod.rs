crate::define_handler_group!(Messaging {
    pub mod fetch_and_decrypt_media;
    pub mod search_messages;
    pub mod send_media;
    pub mod send_message;
    pub mod edit_message;
    pub mod redact_message;
    pub mod send_reaction;
});
