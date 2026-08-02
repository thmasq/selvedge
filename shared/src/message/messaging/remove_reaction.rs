use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RemoveReactionArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub target_event_id: matrix_sdk::ruma::OwnedEventId,
    pub reaction_event_id: Option<matrix_sdk::ruma::OwnedEventId>,
    pub key: String,
}
