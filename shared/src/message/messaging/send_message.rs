use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct SendMessageArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub body: String,
    pub reply_to: Option<matrix_sdk::ruma::OwnedEventId>,
}
