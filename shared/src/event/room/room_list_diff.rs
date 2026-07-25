use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RoomListDiffArgs {
    pub diff: Vec<crate::model::RoomListEntryDiff>,
}
