use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct IdentityUpdatedArgs {
    pub user_id: matrix_sdk::ruma::OwnedUserId,
    pub is_verified: bool,
}
