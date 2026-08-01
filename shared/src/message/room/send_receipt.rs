use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReceiptTypeWrapper {
    Read,
    ReadPrivate,
    FullyRead,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SendReceiptArgs {
    pub request_id: String,
    pub room_id: OwnedRoomId,
    pub event_id: OwnedEventId,
    pub receipt_type: ReceiptTypeWrapper,
}
