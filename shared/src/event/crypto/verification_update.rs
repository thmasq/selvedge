use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct VerificationUpdateArgs {
    pub user_id: matrix_sdk::ruma::OwnedUserId,
    pub flow_id: String,
    pub state: crate::model::VerificationState,
}
