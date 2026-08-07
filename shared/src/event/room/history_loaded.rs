use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryLoadedArgs {
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub start_of_room: bool,
}
