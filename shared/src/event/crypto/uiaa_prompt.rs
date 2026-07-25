use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct UiaaPromptArgs {
    pub request_id: String,
    pub session: String,
}
