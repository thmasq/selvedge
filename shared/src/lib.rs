use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum WorkerCommand {
    Login {
        user: String,
        pass: String,
    },
    Logout,
    InitSync,

    LoadRoom {
        room_id: String,
    },
    LoadHistory {
        room_id: String,
        limit: u32,
    },

    SendMessage {
        room_id: String,
        body: String,
        html: Option<String>,
        reply_to_id: Option<String>,
    },
    SendReaction {
        room_id: String,
        event_id: String,
        key: String,
    },
    EditMessage {
        room_id: String,
        event_id: String,
        new_body: String,
    },
    RedactMessage {
        room_id: String,
        event_id: String,
        reason: Option<String>,
    },

    SetTyping {
        room_id: String,
        typing: bool,
    },
    MarkRead {
        room_id: String,
        event_id: String,
    },

    FetchStickerPacks,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum WorkerEvent {
    LoginSuccess,
    LoggedOut,
    Error(String),

    SyncStatus {
        stage: String,
        progress: Option<f64>,
    },

    RoomListUpdate(Vec<RoomView>),

    TimelineUpdate {
        room_id: String,
        events: Vec<MessageView>,
        clear_cache: bool,
    },

    TypingUpdate {
        room_id: String,
        users: Vec<String>,
    },

    StickerPacksReady(Vec<StickerPackView>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RoomView {
    pub id: String,
    pub name: String,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub typing_users: Vec<String>,
    pub unread_count: u64,
    pub is_direct: bool,
    pub is_encrypted: bool,
    pub members_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MessageView {
    pub id: String,
    pub sender: String,
    pub sender_name: Option<String>,
    pub sender_avatar_url: Option<String>,

    pub body: String,
    pub html_content: Option<String>,

    pub timestamp: u64,
    pub is_mine: bool,
    pub status: MessageStatus,

    pub reply_to_id: Option<String>,
    pub is_edited: bool,

    pub reactions: Vec<ReactionView>,
    pub read_receipts: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReactionView {
    pub key: String,
    pub count: usize,
    pub includes_me: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum MessageStatus {
    Synced,
    Sending,
    Failed,
    Local,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StickerPackView {
    pub name: String,
    pub avatar_url: Option<String>,
    pub stickers: Vec<StickerView>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StickerView {
    pub shortcode: String,
    pub url: String,
    pub body: String,
}
