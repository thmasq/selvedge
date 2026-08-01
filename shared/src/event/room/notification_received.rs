use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationReceivedArgs {
    pub room_id: OwnedRoomId,
    pub event_id: OwnedEventId,
    pub sender: String,
    pub body: String,
}
