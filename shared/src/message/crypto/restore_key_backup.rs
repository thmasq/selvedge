use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RestoreKeyBackupArgs {
    pub request_id: String,
    pub passphrase: String,
}
