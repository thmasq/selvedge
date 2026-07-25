use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct KeysExportedArgs {
    pub request_id: String,
    pub payload: String,
}
