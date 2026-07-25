use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RoomTrustLevelUpdatedArgs {
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub trust_level: crate::model::RoomTrustLevel,
}
