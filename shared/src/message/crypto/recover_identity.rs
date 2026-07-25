use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RecoverIdentityArgs {
    pub request_id: String,
    pub passphrase: String,
}
