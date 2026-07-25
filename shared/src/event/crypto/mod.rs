crate::define_event_group!(Crypto {
    pub mod device_list_result;
    pub mod identity_updated;
    pub mod keys_exported;
    pub mod media_decrypted;
    pub mod qr_code_generated;
    pub mod room_key_request_received;
    pub mod uiaa_prompt;
    pub mod verification_update;
});
