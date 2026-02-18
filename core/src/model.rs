use indexmap::IndexMap;
use matrix_sdk::ruma::{
    MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedMxcUri, OwnedRoomId, OwnedTransactionId,
    OwnedUserId,
    events::room::{ImageInfo, member::MembershipState, message::RoomMessageEventContent},
};
use ruma::events::room::message::AudioInfo;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveCallState {
    pub call_id: String,
    pub participants: HashMap<OwnedUserId, CallParticipant>,
    pub is_connected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallParticipant {
    pub user_id: OwnedUserId,
    pub display_name: Option<String>,
    pub avatar_url: Option<OwnedMxcUri>,
    pub is_speaking: bool,
    pub is_video_muted: bool,
    pub is_audio_muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomSummary {
    pub room_id: OwnedRoomId,
    pub name: Option<String>,
    pub avatar_url: Option<OwnedMxcUri>,
    pub notification_count: u64,
    pub is_direct: bool,
    pub last_message_preview: Option<String>,
    pub last_activity: MilliSecondsSinceUnixEpoch,
    pub has_active_call: bool,
    pub active_call_participant_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomDetails {
    pub room_id: OwnedRoomId,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<OwnedMxcUri>,
    pub members: HashMap<OwnedUserId, MemberProfile>,
    pub timeline: VecDeque<TimelineItem>,
    pub typing_users: HashSet<OwnedUserId>,
    pub active_call: Option<ActiveCallState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberProfile {
    pub user_id: OwnedUserId,
    pub display_name: Option<String>,
    pub avatar_url: Option<OwnedMxcUri>,
    pub membership: MembershipState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum TimelineItem {
    Event(EventItem),
    Virtual(VirtualItem),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventItem {
    pub event_id: OwnedEventId,
    pub sender: OwnedUserId,
    pub sender_profile: Option<MemberProfile>,
    pub timestamp: MilliSecondsSinceUnixEpoch,
    pub content: TimelineContent,
    pub reactions: IndexMap<String, u64>,
    pub delivery_status: DeliveryStatus,
    pub in_reply_to: Option<OwnedEventId>,
    pub is_edited: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeliveryStatus {
    Sending(OwnedTransactionId),
    Sent,
    Error(String),
    Synced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum VirtualItem {
    DayDivider { ts: MilliSecondsSinceUnixEpoch },
    LoadingIndicator,
    TimelineStart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CallType {
    Voice,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msgtype", content = "body")]
pub enum TimelineContent {
    Text {
        body: String,
        formatted: Option<String>,
    },
    Image {
        body: String,
        url: OwnedMxcUri,
        info: Option<ImageInfo>,
    },
    Video {
        body: String,
        url: OwnedMxcUri,
    },
    Audio {
        body: String,
        url: OwnedMxcUri,
        info: Option<AudioInfo>,
    },
    File {
        body: String,
        filename: String,
        url: OwnedMxcUri,
    },
    Notice {
        body: String,
    },
    Emote {
        body: String,
    },
    Call {
        call_type: CallType,
        label: String,
        duration_ms: Option<u64>,
    },
    Unsupported,
    Redacted,
}

fn get_media_url(source: matrix_sdk::ruma::events::room::MediaSource) -> OwnedMxcUri {
    use matrix_sdk::ruma::events::room::MediaSource;
    match source {
        MediaSource::Plain(url) => url,
        MediaSource::Encrypted(file) => file.url,
    }
}

impl From<RoomMessageEventContent> for TimelineContent {
    fn from(content: RoomMessageEventContent) -> Self {
        use matrix_sdk::ruma::events::room::message::MessageType;

        match content.msgtype {
            MessageType::Text(t) => TimelineContent::Text {
                body: t.body,
                formatted: t.formatted.map(|f| f.body),
            },
            MessageType::Image(i) => TimelineContent::Image {
                body: i.body,
                url: get_media_url(i.source),
                info: i.info.map(|b| *b),
            },
            MessageType::Video(v) => TimelineContent::Video {
                body: v.body,
                url: get_media_url(v.source),
            },
            MessageType::Audio(a) => TimelineContent::Audio {
                body: a.body,
                url: get_media_url(a.source),
                info: a.info.map(|b| *b),
            },
            MessageType::File(f) => TimelineContent::File {
                body: f.body,
                filename: f.filename.unwrap_or_default(),
                url: get_media_url(f.source),
            },
            MessageType::Notice(n) => TimelineContent::Notice { body: n.body },
            MessageType::Emote(e) => TimelineContent::Emote { body: e.body },
            _ => TimelineContent::Unsupported,
        }
    }
}
