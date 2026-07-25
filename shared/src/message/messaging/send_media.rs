use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct SendMediaArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}
