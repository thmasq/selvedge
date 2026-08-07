crate::define_message_group!(Room {
    pub mod clear_room_warning;
    pub mod close_room;
    pub mod create_room;
    pub mod join_room;
    pub mod leave_room;
    pub mod load_history;
    pub mod load_room_members;
    pub mod open_room;
    pub mod set_typing;
    pub mod send_receipt;
    pub mod search_room_members;
});
