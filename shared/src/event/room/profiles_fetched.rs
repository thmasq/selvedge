use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfilesFetchedArgs {
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub profiles: std::collections::HashMap<matrix_sdk::ruma::OwnedUserId, crate::MemberProfile>,
}
