crate::define_event_group!(Room {
    pub mod profiles_fetched;
    pub mod room_details_update;
    pub mod room_list_diff;
    pub mod room_members_loaded;
    pub mod room_trust_level_updated;
    pub mod timeline_diff;
    pub mod typing_updated;
    pub mod notification_received;
    pub mod room_members_searched;
    pub mod history_loaded;
});
