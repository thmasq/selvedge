use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct TimelineDiffArgs {
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub diff: Vec<crate::model::TimelineDiff>,
}
