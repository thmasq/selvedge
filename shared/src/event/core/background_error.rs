use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct BackgroundErrorArgs {
    pub error: crate::model::ActorError,
}
