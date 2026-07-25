use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResultArgs {
    pub request_id: String,
    pub success: bool,
    pub error: Option<crate::model::ActorError>,
}
