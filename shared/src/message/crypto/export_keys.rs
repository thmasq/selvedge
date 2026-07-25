use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportKeysArgs {
    pub request_id: String,
    pub passphrase: String,
}
