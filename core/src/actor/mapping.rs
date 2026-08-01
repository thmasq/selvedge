use eyeball_im::VectorDiff;
use indexmap::IndexMap;
use matrix_sdk::Client;
use matrix_sdk::ruma::events::room::member::MembershipState;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{MilliSecondsSinceUnixEpoch, OwnedEventId};
use matrix_sdk_ui::room_list_service::RoomListItem;
use matrix_sdk_ui::timeline::{
    EmbeddedEvent, EventSendState, EventTimelineItem, MsgLikeContent, TimelineDetails,
    TimelineEventShieldState, TimelineItemContent, TimelineItemKind, VirtualTimelineItem,
};
use ruma::presence::PresenceState;
use selvedge_shared::{
    DeliveryStatus, EncryptionStatus, EventItem, ModelError, RoomListEntryDiff, RoomListEntryView,
    RoomSummary, TimelineContent, TimelineDiff, TimelineItem, VirtualItem,
};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

// ---------- content resolution ----------

fn map_message_content(event: &EventTimelineItem) -> TimelineContent {
    match event.content() {
        TimelineItemContent::MsgLike(msg) => match &msg.kind {
            matrix_sdk_ui::timeline::MsgLikeKind::Message(msg_content) => {
                RoomMessageEventContent::new(msg_content.msgtype().clone()).into()
            }
            matrix_sdk_ui::timeline::MsgLikeKind::Redacted => TimelineContent::Redacted,
            _ => TimelineContent::Unsupported,
        },
        _ => TimelineContent::Unsupported,
    }
}

fn extract_undecryptable_content(event: &EventTimelineItem) -> Option<TimelineContent> {
    let raw_json = event.latest_json()?;
    let event_type = raw_json.get_field::<String>("type").ok().flatten()?;
    if event_type != "m.room.encrypted" {
        return None;
    }

    let content_val = raw_json
        .get_field::<serde_json::Value>("content")
        .ok()
        .flatten();

    let session_id = content_val
        .as_ref()
        .and_then(|c| c.get("session_id"))
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();

    let sender_key = content_val
        .as_ref()
        .and_then(|c| c.get("sender_key"))
        .and_then(|s| s.as_str())
        .unwrap_or_default()
        .to_string();

    Some(TimelineContent::Undecryptable {
        session_id,
        sender_key,
    })
}

fn resolve_content(event: &EventTimelineItem) -> TimelineContent {
    let content = map_message_content(event);
    if matches!(content, TimelineContent::Unsupported) {
        extract_undecryptable_content(event).unwrap_or(content)
    } else {
        content
    }
}

// ---------- delivery / identity ----------

fn compute_delivery_status(event: &EventTimelineItem) -> DeliveryStatus {
    event
        .send_state()
        .map_or(DeliveryStatus::Synced, |local_echo| match local_echo {
            EventSendState::NotSentYet { .. } => {
                DeliveryStatus::Sending(matrix_sdk::ruma::OwnedTransactionId::from("dummy"))
            }
            EventSendState::Sent { .. } => DeliveryStatus::Sent,
            EventSendState::SendingFailed { .. } => {
                DeliveryStatus::Error(ModelError::DeliveryFailed("Failed to send".to_string()))
            }
        })
}

fn resolve_event_id(event: &EventTimelineItem) -> matrix_sdk::ruma::OwnedEventId {
    event.event_id().map_or_else(
        || matrix_sdk::ruma::OwnedEventId::from_str("$dummy").unwrap(),
        std::borrow::ToOwned::to_owned,
    )
}

async fn build_sender_profile(
    client: &Client,
    event: &EventTimelineItem,
) -> Option<selvedge_shared::MemberProfile> {
    let TimelineDetails::Ready(profile) = event.sender_profile() else {
        return None;
    };

    let is_verified = client
        .encryption()
        .get_user_identity(event.sender())
        .await
        .ok()
        .flatten()
        .is_some_and(|identity| identity.is_verified());

    Some(selvedge_shared::MemberProfile {
        user_id: event.sender().to_owned(),
        display_name: profile.display_name.clone(),
        avatar_url: profile.avatar_url.clone(),
        membership: MembershipState::Join,
        presence: PresenceState::Unavailable,
        is_verified,
    })
}

// ---------- reactions / replies ----------

fn extract_reactions(
    client: &Client,
    msg_like: &MsgLikeContent,
) -> IndexMap<String, selvedge_shared::model::ReactionDetails> {
    let mut reactions = IndexMap::new();
    for (emoji, users) in msg_like.reactions.iter() {
        let count = users.len() as u64;
        let me_reacted = client
            .user_id()
            .is_some_and(|my_id| users.contains_key(my_id));

        reactions.insert(
            emoji.clone(),
            selvedge_shared::model::ReactionDetails { count, me_reacted },
        );
    }
    reactions
}

fn build_reply_details(replied_event: &EmbeddedEvent) -> selvedge_shared::model::ReplyDetails {
    let reply_content = Box::new(match &replied_event.content {
        TimelineItemContent::MsgLike(rl_msg) => rl_msg
            .as_message()
            .map_or(TimelineContent::Unsupported, |m| {
                TimelineContent::from(RoomMessageEventContent::new(m.msgtype().clone()))
            }),
        _ => TimelineContent::Unsupported,
    });

    selvedge_shared::model::ReplyDetails {
        sender: replied_event.sender.clone(),
        sender_display_name: match &replied_event.sender_profile {
            TimelineDetails::Ready(p) => p.display_name.clone(),
            _ => None,
        },
        content: reply_content,
    }
}

struct ReplyInfo {
    in_reply_to: Option<OwnedEventId>,
    reply_details: Option<selvedge_shared::model::ReplyDetails>,
    thread_root_id: Option<OwnedEventId>,
}

fn extract_reply_info(msg_like: &MsgLikeContent) -> ReplyInfo {
    let thread_root_id = msg_like.thread_root.clone();

    let Some(reply_info) = &msg_like.in_reply_to else {
        return ReplyInfo {
            in_reply_to: None,
            reply_details: None,
            thread_root_id,
        };
    };

    let reply_details = match &reply_info.event {
        TimelineDetails::Ready(replied_event) => Some(build_reply_details(replied_event)),
        _ => None,
    };

    ReplyInfo {
        in_reply_to: Some(reply_info.event_id.clone()),
        reply_details,
        thread_root_id,
    }
}

// ---------- encryption ----------

fn compute_encryption_status(event: &EventTimelineItem) -> EncryptionStatus {
    match event.get_shield(false) {
        TimelineEventShieldState::None => {
            if event.encryption_info().is_some() {
                EncryptionStatus::Verified
            } else {
                EncryptionStatus::Unencrypted
            }
        }
        _ => EncryptionStatus::Unverified,
    }
}

// ---------- event/virtual orchestration ----------

async fn map_event_item(client: &Client, event: &EventTimelineItem) -> EventItem {
    let content = resolve_content(event);
    let delivery_status = compute_delivery_status(event);
    let event_id = resolve_event_id(event);
    let sender_profile = build_sender_profile(client, event).await;

    let (reactions, reply_info) = match event.content() {
        TimelineItemContent::MsgLike(msg_like) => (
            extract_reactions(client, msg_like),
            extract_reply_info(msg_like),
        ),
        _ => (
            IndexMap::new(),
            ReplyInfo {
                in_reply_to: None,
                reply_details: None,
                thread_root_id: None,
            },
        ),
    };

    let encryption_status = compute_encryption_status(event);
    let is_edited = event
        .content()
        .as_message()
        .is_some_and(matrix_sdk_ui::timeline::Message::is_edited);

    EventItem {
        event_id,
        sender: event.sender().to_owned(),
        sender_profile,
        timestamp: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(event.timestamp().0),
        content: Box::new(content),
        reactions,
        read_receipts: event.read_receipts().keys().cloned().collect(),
        delivery_status,
        in_reply_to: reply_info.in_reply_to,
        reply_details: reply_info.reply_details,
        is_edited,
        latest_edit: None,
        thread_root_id: reply_info.thread_root_id,
        is_highlight: event.is_highlighted(),
        should_group: false,
        encryption_status,
    }
}

const fn map_virtual_item(virt: &VirtualTimelineItem) -> VirtualItem {
    match virt {
        VirtualTimelineItem::DateDivider(ts) => VirtualItem::DayDivider {
            ts: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(ts.0),
        },
        _ => VirtualItem::LoadingIndicator,
    }
}

pub async fn map_timeline_item_safe(
    client: &Client,
    item: &matrix_sdk_ui::timeline::TimelineItem,
) -> TimelineItem {
    match item.kind() {
        TimelineItemKind::Event(event) => {
            TimelineItem::Event(Box::new(map_event_item(client, event).await))
        }
        TimelineItemKind::Virtual(virt) => TimelineItem::Virtual(map_virtual_item(virt)),
    }
}

pub async fn map_timeline_diff(
    client: &Client,
    diff: VectorDiff<Arc<matrix_sdk_ui::timeline::TimelineItem>>,
) -> TimelineDiff {
    match diff {
        VectorDiff::Append { values } => {
            let mut entries = Vec::new();
            for v in values {
                entries.push(map_timeline_item_safe(client, &v).await);
            }
            TimelineDiff::Append { entries }
        }
        VectorDiff::Clear => TimelineDiff::Clear,
        VectorDiff::PushFront { value } => TimelineDiff::PushFront {
            entry: map_timeline_item_safe(client, &value).await,
        },
        VectorDiff::PushBack { value } => TimelineDiff::PushBack {
            entry: map_timeline_item_safe(client, &value).await,
        },
        VectorDiff::PopFront => TimelineDiff::PopFront,
        VectorDiff::PopBack => TimelineDiff::PopBack,
        VectorDiff::Insert { index, value } => TimelineDiff::Insert {
            index,
            entry: map_timeline_item_safe(client, &value).await,
        },
        VectorDiff::Set { index, value } => TimelineDiff::Set {
            index,
            entry: map_timeline_item_safe(client, &value).await,
        },
        VectorDiff::Remove { index } => TimelineDiff::Remove { index },
        VectorDiff::Truncate { length } => TimelineDiff::Truncate { length },
        VectorDiff::Reset { values } => {
            let mut entries = Vec::new();
            for v in values {
                entries.push(map_timeline_item_safe(client, &v).await);
            }
            TimelineDiff::Reset { entries }
        }
    }
}

// ---------- room list ----------

fn compute_last_activity(client: &Client, item: &RoomListItem) -> MilliSecondsSinceUnixEpoch {
    client
        .get_room(item.room_id())
        .and_then(|room| room.latest_event_timestamp())
        .unwrap_or_else(|| MilliSecondsSinceUnixEpoch(0u32.into()))
}

fn is_room_encrypted(room: Option<&matrix_sdk::Room>) -> bool {
    room.as_ref()
        .is_some_and(|r| r.encryption_state().is_encrypted())
}

async fn build_room_summary(client: &Client, item: &RoomListItem) -> RoomSummary {
    let unread = item.unread_notification_counts();
    let last_activity = compute_last_activity(client, item);
    let room = client.get_room(item.room_id());
    let receipts = item.read_receipts();
    let is_encrypted = is_room_encrypted(room.as_ref());

    RoomSummary {
        room_id: item.room_id().to_owned(),
        name: item.name(),
        avatar_url: item.avatar_url(),
        notification_count: unread.notification_count,
        highlight_count: unread.highlight_count,
        unread_count: receipts.num_unread,
        is_direct: item.is_direct().await.unwrap_or(false),
        last_message_preview: None,
        last_activity,
        has_active_call: false,
        active_call_participant_count: 0,
        is_encrypted,
        tags: HashSet::new(),
    }
}

#[allow(clippy::future_not_send)]
pub async fn room_list_item_to_view(client: Client, item: RoomListItem) -> RoomListEntryView {
    RoomListEntryView::Filled(build_room_summary(&client, &item).await)
}

#[allow(clippy::future_not_send)]
pub async fn map_room_list_diff(
    client: Client,
    diff: VectorDiff<RoomListItem>,
) -> RoomListEntryDiff {
    match diff {
        VectorDiff::Append { values } => RoomListEntryDiff::Append {
            entries: futures::future::join_all(
                values
                    .into_iter()
                    .map(|v| room_list_item_to_view(client.clone(), v)),
            )
            .await,
        },
        VectorDiff::Clear => RoomListEntryDiff::Clear,
        VectorDiff::PushFront { value } => RoomListEntryDiff::PushFront {
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::PushBack { value } => RoomListEntryDiff::PushBack {
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::PopFront => RoomListEntryDiff::PopFront,
        VectorDiff::PopBack => RoomListEntryDiff::PopBack,
        VectorDiff::Insert { index, value } => RoomListEntryDiff::Insert {
            index,
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::Set { index, value } => RoomListEntryDiff::Set {
            index,
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::Remove { index } => RoomListEntryDiff::Remove { index },
        VectorDiff::Truncate { length } => RoomListEntryDiff::Truncate { length },
        VectorDiff::Reset { values } => RoomListEntryDiff::Reset {
            entries: futures::future::join_all(
                values
                    .into_iter()
                    .map(|v| room_list_item_to_view(client.clone(), v)),
            )
            .await,
        },
    }
}
