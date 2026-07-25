use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchMessagesArgs {
    pub request_id: String,
    pub room_id: Option<matrix_sdk::ruma::OwnedRoomId>,
    pub query: String,
    pub limit: usize,
}
