use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct LogoutArgs {
    pub request_id: String,
}
