use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct EnableKeyBackupArgs {
    pub request_id: String,
    pub passphrase: String,
}
