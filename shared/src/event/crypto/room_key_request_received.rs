use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RoomKeyRequestReceivedArgs {
    pub request_id: String,
    pub requester_user_id: matrix_sdk::ruma::OwnedUserId,
    pub requester_device_id: String,
}
