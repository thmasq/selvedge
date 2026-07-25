use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResultsArgs {
    pub request_id: String,
    pub results: Vec<crate::model::EventItem>,
}
