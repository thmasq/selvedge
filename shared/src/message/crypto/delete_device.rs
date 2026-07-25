use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteDeviceArgs {
    pub request_id: String,
    pub device_id: String,
    pub uia_session: Option<String>,
    pub password: Option<String>,
}
