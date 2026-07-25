use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct SubmitUiaResponseArgs {
    pub request_id: String,
    pub session: String,
    pub password: String,
    pub passphrase: String,
}
