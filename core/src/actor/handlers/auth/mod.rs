crate::define_handler_group!(Auth {
    pub mod login;
    pub mod logout;
    pub mod restore_session;
});
