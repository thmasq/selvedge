use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct JoinRoomArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
}
