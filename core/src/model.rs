#![allow(clippy::cast_possible_truncation)]

use indexmap::IndexMap;
use matrix_sdk::ruma::{
    MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedMxcUri, OwnedRoomId, OwnedTransactionId,
    OwnedUserId,
    events::{
        key::verification::VerificationMethod,
        room::{
            MediaSource as RumaMediaSource,
            member::MembershipState,
            message::{MessageType, RoomMessageEventContent},
        },
    },
};
use ruma::events::room::EncryptedFile;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq, Eq)]
pub enum ModelError {
    #[error("Failed to deliver message: {0}")]
    DeliveryFailed(String),

    #[error("Unable to decrypt message: {0}")]
    Undecryptable(String),

    #[error("Unsupported event type encountered")]
    UnsupportedEvent,

    #[error("Media conversion error: {0}")]
    MediaError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq, Eq)]
pub enum ActorError {
    #[error("Failed to authenticate: {0}")]
    LoginFailed(String),

    #[error("Failed to start sync service: {0}")]
    SyncInitializationFailed(String),

    #[error("Room operation failed: {0}")]
    RoomOperationFailed(String),

    #[error("Pagination failed: {0}")]
    PaginationFailed(String),

    #[error("Client is not initialized")]
    ClientNotInitialized,
}

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
pub enum TimelineDiff {
    Append { entries: Vec<TimelineItem> },
    Clear,
    PushFront { entry: TimelineItem },
    PushBack { entry: TimelineItem },
    PopFront,
    PopBack,
    Insert { index: usize, entry: TimelineItem },
    Set { index: usize, entry: TimelineItem },
    Remove { index: usize },
    Truncate { length: usize },
    Reset { entries: Vec<TimelineItem> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoomListEntryView {
    Empty,
    Invalidated(OwnedRoomId),
    Filled(RoomSummary),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RoomListEntryDiff {
    Append {
        entries: Vec<RoomListEntryView>,
    },
    Clear,
    PushFront {
        entry: RoomListEntryView,
    },
    PushBack {
        entry: RoomListEntryView,
    },
    PopFront,
    PopBack,
    Insert {
        index: usize,
        entry: RoomListEntryView,
    },
    Set {
        index: usize,
        entry: RoomListEntryView,
    },
    Remove {
        index: usize,
    },
    Truncate {
        length: usize,
    },
    Reset {
        entries: Vec<RoomListEntryView>,
    },
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
    pub unread_count: u64,
    pub highlight_count: u64,

    pub is_encrypted: bool,
    pub tags: HashSet<String>,
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

    pub is_encrypted: bool,

    pub permissions: RoomPermissions,
    pub prev_batch: Option<String>,
    pub next_batch: Option<String>,
    pub fully_read_marker: Option<OwnedEventId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "content")]
pub enum TimelineItem {
    Event(EventItem),
    Virtual(VirtualItem),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyDetails {
    pub sender: OwnedUserId,
    pub sender_display_name: Option<String>,
    pub content: Box<TimelineContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventItem {
    pub event_id: OwnedEventId,
    pub sender: OwnedUserId,
    pub sender_profile: Option<MemberProfile>,
    pub timestamp: MilliSecondsSinceUnixEpoch,

    pub content: Box<TimelineContent>,

    pub reactions: IndexMap<String, ReactionDetails>,
    pub read_receipts: Vec<OwnedUserId>,
    pub delivery_status: DeliveryStatus,

    pub in_reply_to: Option<OwnedEventId>,
    pub reply_details: Option<ReplyDetails>,
    pub is_edited: bool,
    pub latest_edit: Option<Box<TimelineContent>>,
    pub thread_root_id: Option<OwnedEventId>,

    pub is_highlight: bool,
    pub should_group: bool,
    pub encryption_status: EncryptionStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum TimelineContent {
    Message(MessageContent),
    State(StateContent),
    Poll(PollState),
    Call(CallContent),
    Verification(VerificationRequest),
    Redaction { reason: Option<String> },
    Unsupported,
    Redacted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "msgtype", content = "data")]
pub enum MessageContent {
    Text {
        body: String,
        formatted: Option<String>,
        previews: Vec<LinkPreview>,
    },
    Image {
        body: String,
        source: MediaSource,
        info: Option<ImageInfo>,
    },
    Video {
        body: String,
        source: MediaSource,
        info: Option<VideoInfo>,
    },
    Audio {
        body: String,
        source: MediaSource,
        info: Option<AudioInfoWrapper>,
    },
    File {
        body: String,
        filename: String,
        source: MediaSource,
    },
    Sticker {
        body: String,
        source: MediaSource,
        info: Option<ImageInfo>,
    },
    Notice {
        body: String,
        formatted: Option<String>,
    },
    Emote {
        body: String,
        formatted: Option<String>,
    },
    Location {
        body: String,
        geo_uri: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StateContent {
    Member {
        user_id: OwnedUserId,
        membership: MembershipState,
        prev_membership: Option<MembershipState>,
        reason: Option<String>,
    },
    RoomName {
        name: String,
    },
    RoomTopic {
        topic: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallContent {
    pub call_id: String,
    pub call_type: CallType,
    pub duration: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaSource {
    Plain(OwnedMxcUri),
    Encrypted(Box<EncryptedFile>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageInfo {
    pub height: Option<u64>,
    pub width: Option<u64>,
    pub mimetype: Option<String>,
    pub size: Option<u64>,
    pub thumbnail_source: Option<MediaSource>,
    pub thumbnail_info: Option<Box<ThumbnailInfo>>,
    pub blurhash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInfo {
    pub duration: Option<u64>,
    pub height: Option<u64>,
    pub width: Option<u64>,
    pub mimetype: Option<String>,
    pub size: Option<u64>,
    pub thumbnail_source: Option<MediaSource>,
    pub blurhash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailInfo {
    pub height: Option<u64>,
    pub width: Option<u64>,
    pub mimetype: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkPreview {
    pub url: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub image: Option<MediaSource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberProfile {
    pub user_id: OwnedUserId,
    pub display_name: Option<String>,
    pub avatar_url: Option<OwnedMxcUri>,
    pub membership: MembershipState,
    pub presence: PresenceState,

    pub is_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum PresenceState {
    Online,
    #[default]
    Offline,
    Unavailable,
    Unknown,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoomPermissions {
    pub can_send_message: bool,
    pub can_send_media: bool,
    pub can_redact: bool,
    pub can_ban: bool,
    pub can_kick: bool,
    pub can_invite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionDetails {
    pub count: u64,
    pub me_reacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeliveryStatus {
    Sending(OwnedTransactionId),
    Sent,
    Error(ModelError),
    Synced,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EncryptionStatus {
    Unencrypted,
    Verified,
    Unverified,
    Undecryptable(ModelError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioInfoWrapper {
    pub duration: Option<u64>,
    pub mimetype: Option<String>,
    pub size: Option<u64>,
    pub waveform: Option<Vec<u16>>,
    pub is_voice_message: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollState {
    pub question: String,
    pub answers: Vec<PollAnswer>,
    pub is_closed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PollAnswer {
    pub id: String,
    pub text: String,
    pub count: u64,
    pub is_selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationRequest {
    pub body: String,
    pub from_device: Option<String>,
    pub state: VerificationState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationState {
    Requested { methods: Vec<VerificationMethod> },
    Ready,
    Started { method: VerificationMethod },
    SasEmoji { emoji: Vec<(String, String)> },
    SasDecimal { decimals: (u16, u16, u16) },
    Cancelled,
    Done,
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

impl From<RumaMediaSource> for MediaSource {
    fn from(source: RumaMediaSource) -> Self {
        match source {
            RumaMediaSource::Plain(url) => Self::Plain(url),
            RumaMediaSource::Encrypted(file) => Self::Encrypted(Box::new(*file)),
        }
    }
}

impl From<matrix_sdk::ruma::events::room::ImageInfo> for ImageInfo {
    fn from(info: matrix_sdk::ruma::events::room::ImageInfo) -> Self {
        Self {
            height: info.height.map(std::convert::Into::into),
            width: info.width.map(std::convert::Into::into),
            mimetype: info.mimetype,
            size: info.size.map(std::convert::Into::into),
            thumbnail_source: info.thumbnail_source.map(Into::into),
            thumbnail_info: info.thumbnail_info.map(|t| {
                Box::new(ThumbnailInfo {
                    height: t.height.map(std::convert::Into::into),
                    width: t.width.map(std::convert::Into::into),
                    mimetype: t.mimetype,
                    size: t.size.map(std::convert::Into::into),
                })
            }),
            blurhash: info.blurhash,
        }
    }
}

impl From<RoomMessageEventContent> for TimelineContent {
    fn from(content: RoomMessageEventContent) -> Self {
        match content.msgtype {
            MessageType::Text(t) => Self::Message(MessageContent::Text {
                body: t.body,
                formatted: t.formatted.map(|f| f.body),
                previews: Vec::new(),
            }),
            MessageType::Image(i) => Self::Message(MessageContent::Image {
                body: i.body,
                source: i.source.into(),
                info: i.info.map(|info| (*info).into()),
            }),
            MessageType::Video(v) => Self::Message(MessageContent::Video {
                body: v.body,
                source: v.source.into(),
                info: v.info.map(|info| VideoInfo {
                    duration: info.duration.map(|d| d.as_millis() as u64),
                    height: info.height.map(std::convert::Into::into),
                    width: info.width.map(std::convert::Into::into),
                    mimetype: info.mimetype,
                    size: info.size.map(std::convert::Into::into),
                    thumbnail_source: info.thumbnail_source.map(Into::into),
                    blurhash: info.blurhash,
                }),
            }),
            MessageType::Audio(a) => Self::Message(MessageContent::Audio {
                body: a.body,
                source: a.source.into(),
                info: a.info.map(|info| AudioInfoWrapper {
                    duration: info.duration.map(|d| d.as_millis() as u64),
                    mimetype: info.mimetype,
                    size: info.size.map(std::convert::Into::into),
                    waveform: None,
                    is_voice_message: false,
                }),
            }),
            MessageType::File(f) => Self::Message(MessageContent::File {
                body: f.body,
                filename: f.filename.unwrap_or_default(),
                source: f.source.into(),
            }),
            MessageType::Notice(n) => Self::Message(MessageContent::Notice {
                body: n.body,
                formatted: n.formatted.map(|f| f.body),
            }),
            MessageType::Emote(e) => Self::Message(MessageContent::Emote {
                body: e.body,
                formatted: e.formatted.map(|f| f.body),
            }),
            MessageType::Location(l) => Self::Message(MessageContent::Location {
                body: l.body,
                geo_uri: l.geo_uri,
            }),
            MessageType::VerificationRequest(v) => Self::Verification(VerificationRequest {
                body: v.body,
                from_device: Some(v.from_device.to_string()),
                state: VerificationState::Requested { methods: v.methods },
            }),
            _ => Self::Unsupported,
        }
    }
}
