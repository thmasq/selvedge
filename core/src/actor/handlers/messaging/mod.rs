crate::define_handler_group!(Messaging {
    pub mod fetch_and_decrypt_media;
    pub mod search_messages;
    pub mod send_media;
    pub mod send_message;
});
