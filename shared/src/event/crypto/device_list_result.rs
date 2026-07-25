use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct DeviceListResultArgs {
    pub request_id: String,
    pub devices: Vec<crate::model::DeviceInfo>,
}
