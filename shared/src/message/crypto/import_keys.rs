use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct ImportKeysArgs {
    pub request_id: String,
    pub passphrase: String,
    pub payload: Vec<u8>,
}
