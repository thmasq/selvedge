crate::define_handler_group!(Crypto {
    pub mod accept_verification; pub mod cancel_verification; pub mod confirm_qr_scan;
    pub mod confirm_verification; pub mod delete_device; pub mod enable_key_backup;
    pub mod export_keys; pub mod generate_qr_code; pub mod get_my_devices;
    pub mod import_keys; pub mod recover_identity; pub mod request_room_key;
    pub mod request_verification; pub mod restore_key_backup; pub mod retry_decryption;
    pub mod setup_recovery; pub mod submit_uia_response;
});
