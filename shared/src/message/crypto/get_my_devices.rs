use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct GetMyDevicesArgs {
    pub request_id: String,
}
