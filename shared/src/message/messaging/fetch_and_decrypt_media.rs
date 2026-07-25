use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct FetchAndDecryptMediaArgs {
    pub request_id: String,
    pub source: crate::model::MediaSource,
    pub mime_type: String,
}
