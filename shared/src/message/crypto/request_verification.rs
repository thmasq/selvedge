use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct RequestVerificationArgs {
    pub request_id: String,
    pub user_id: matrix_sdk::ruma::OwnedUserId,
}
