use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RedactMessageArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub event_id: matrix_sdk::ruma::OwnedEventId,
    pub reason: Option<String>,
}
