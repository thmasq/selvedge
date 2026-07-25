use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginFailureArgs {
    pub error: crate::model::ActorError,
}
