use crate::model::MemberProfile;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct RoomMembersSearchedArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub query: String,
    pub results: Vec<MemberProfile>,
}
