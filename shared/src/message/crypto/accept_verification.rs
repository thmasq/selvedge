use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptVerificationArgs {
    pub request_id: String,
    pub user_id: matrix_sdk::ruma::OwnedUserId,
    pub flow_id: String,
}
