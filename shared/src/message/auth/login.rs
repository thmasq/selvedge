use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct LoginArgs {
    pub homeserver_url: String,
    pub username: String,
    pub password: String,
}
