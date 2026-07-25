use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct ClearRoomWarningArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub user_id: matrix_sdk::ruma::OwnedUserId,
}
