use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RoomMembersLoadedArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub members: std::collections::HashMap<matrix_sdk::ruma::OwnedUserId, crate::MemberProfile>,
}
