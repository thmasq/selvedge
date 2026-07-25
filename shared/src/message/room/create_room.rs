use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateRoomArgs {
    pub request_id: String,
    pub name: String,
    pub topic: Option<String>,
    pub is_encrypted: bool,
}
