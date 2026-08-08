use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId};
use serde::{Deserialize, Serialize};

/// When focus_event_id is None, goes to the live edge
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenRoomArgs {
    pub room_id: OwnedRoomId,
    pub focus_event_id: Option<OwnedEventId>,
}
