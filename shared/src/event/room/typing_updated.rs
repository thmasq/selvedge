use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct TypingUpdatedArgs {
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub typing_users: Vec<matrix_sdk::ruma::OwnedUserId>,
}
