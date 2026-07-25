use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct LoadHistoryArgs {
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
}
