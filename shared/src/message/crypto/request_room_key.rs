use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestRoomKeyArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub session_id: String,
    pub sender_key: String,
}
