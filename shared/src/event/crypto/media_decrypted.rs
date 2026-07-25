use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct MediaDecryptedArgs {
    pub request_id: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}
