use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct QrCodeGeneratedArgs {
    pub request_id: String,
    pub payload: Vec<u8>,
}
