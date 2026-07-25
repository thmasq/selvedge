use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RoomDetailsUpdateArgs {
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub details: crate::model::RoomDetails,
}
