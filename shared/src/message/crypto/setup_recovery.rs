use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct SetupRecoveryArgs {
    pub request_id: String,
    pub passphrase: String,
}
