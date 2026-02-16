use matrix_sdk::ruma::{
    EventId, OwnedEventId, OwnedRoomId, OwnedUserId,
    events::{
        AnySyncStateEvent,
        receipt::ReceiptThread,
        room::{
            encrypted::RedactedRoomEncryptedEvent,
            message::{RedactedRoomMessageEvent, RoomMessageEventContent},
        },
    },
};
use ruma::events::{SyncMessageLikeEvent, room::encrypted::RoomEncryptedEventContent};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageTimeStamp {
    OriginServer(ruma::MilliSecondsSinceUnixEpoch),
    LocalEcho,
}

#[derive(Clone, Debug)]
pub enum MessageEvent {
    EncryptedOriginal(Box<SyncMessageLikeEvent<RoomEncryptedEventContent>>),
    EncryptedRedacted(Box<RedactedRoomEncryptedEvent>),
    Original(Box<SyncMessageLikeEvent<RoomMessageEventContent>>),
    Redacted(Box<RedactedRoomMessageEvent>),
    State(Box<AnySyncStateEvent>),
    Local(OwnedEventId, Box<RoomMessageEventContent>),
}

impl MessageEvent {
    pub fn event_id(&self) -> &EventId {
        match self {
            MessageEvent::EncryptedOriginal(ev) => &ev.event_id(),
            MessageEvent::EncryptedRedacted(ev) => &ev.event_id,
            MessageEvent::Original(ev) => &ev.event_id(),
            MessageEvent::Redacted(ev) => &ev.event_id,
            MessageEvent::State(ev) => ev.event_id(),
            MessageEvent::Local(id, _) => id,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Message {
    pub event: MessageEvent,
    pub sender: OwnedUserId,
    pub timestamp: MessageTimeStamp,
    pub formatted_body: Option<String>,
    pub downloaded: bool,
}

pub type MessageKey = (MessageTimeStamp, OwnedEventId);

#[derive(Clone, Debug, Default)]
pub struct Messages {
    pub map: BTreeMap<MessageKey, Message>,
    pub thread_root: Option<OwnedEventId>,
}

impl Messages {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Clone, Debug)]
pub enum EventLocation {
    Message(Option<OwnedEventId>, MessageKey),
    Reaction(OwnedEventId),
    State(MessageKey),
}

#[derive(Clone, Debug, Default)]
pub struct RoomInfo {
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub is_direct: bool,
    pub is_encrypted: bool,

    pub notification_count: u64,
    pub highlight_count: u64,
    pub active_members_count: u64,

    pub keys: HashMap<OwnedEventId, EventLocation>,
    pub messages: Messages,
    pub threads: HashMap<OwnedEventId, Messages>,

    pub event_receipts: HashMap<ReceiptThread, HashMap<OwnedEventId, HashSet<OwnedUserId>>>,
    pub user_receipts: HashMap<ReceiptThread, HashMap<OwnedUserId, OwnedEventId>>,

    pub reactions: HashMap<OwnedEventId, HashMap<String, HashSet<OwnedUserId>>>,

    pub users_typing: Option<(u64, Vec<OwnedUserId>)>,

    pub display_names: HashMap<OwnedUserId, String>,
    pub user_avatars: HashMap<OwnedUserId, String>,
}

#[derive(Default)]
pub struct ChatStore {
    pub rooms: HashMap<OwnedRoomId, RoomInfo>,

    pub emoji_packs: HashMap<String, String>,
    pub sticker_packs: Vec<StickerPack>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StickerPack {
    pub name: String,
    pub avatar_url: Option<String>,
    pub images: HashMap<String, String>,
}
