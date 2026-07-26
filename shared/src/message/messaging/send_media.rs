use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct SendMediaArgs {
    pub request_id: String,
    pub room_id: matrix_sdk::ruma::OwnedRoomId,
    pub filename: String,
    pub mime_type: String,
    pub data: Vec<u8>,

    pub caption: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub blurhash: Option<String>,
    pub thumbnail_data: Option<Vec<u8>>,
    pub thumbnail_mime: Option<String>,
}
