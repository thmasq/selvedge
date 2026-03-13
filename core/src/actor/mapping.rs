use eyeball_im::VectorDiff;
use indexmap::IndexMap;
use matrix_sdk::Client;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{MilliSecondsSinceUnixEpoch, OwnedEventId, OwnedTransactionId};
use matrix_sdk_ui::room_list_service::RoomListItem;
use matrix_sdk_ui::timeline::{
    EventSendState, Message, TimelineDetails, TimelineItemContent, TimelineItemKind,
    VirtualTimelineItem,
};
use selvedge_shared::{
    DeliveryStatus, EncryptionStatus, EventItem, ModelError, RoomListEntryDiff, RoomListEntryView,
    RoomSummary, TimelineContent, TimelineDiff, TimelineItem, VirtualItem,
};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

pub(crate) async fn map_timeline_item_safe(
    client: &Client,
    item: &matrix_sdk_ui::timeline::TimelineItem,
) -> TimelineItem {
    match item.kind() {
        TimelineItemKind::Event(event) => {
            let mut content = match event.content() {
                TimelineItemContent::MsgLike(msg) => {
                    if let Some(msg_content) = msg.as_message() {
                        let ruma_content =
                            RoomMessageEventContent::new(msg_content.msgtype().clone());
                        ruma_content.into()
                    } else {
                        TimelineContent::Unsupported
                    }
                }
                _ => TimelineContent::Unsupported,
            };

            if matches!(content, TimelineContent::Unsupported) {
                if let Some(raw_json) = event.latest_json() {
                    if let Ok(Some(event_type)) = raw_json.get_field::<String>("type") {
                        if event_type == "m.room.encrypted" {
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

                            content = TimelineContent::Undecryptable {
                                session_id,
                                sender_key,
                            };
                        }
                    }
                }
            }

            let delivery_status = event
                .send_state()
                .map_or(DeliveryStatus::Synced, |local_echo| match local_echo {
                    EventSendState::NotSentYet { .. } => {
                        DeliveryStatus::Sending(OwnedTransactionId::from("dummy"))
                    }
                    EventSendState::Sent { .. } => DeliveryStatus::Sent,
                    EventSendState::SendingFailed { .. } => DeliveryStatus::Error(
                        ModelError::DeliveryFailed("Failed to send".to_string()),
                    ),
                });

            let event_id = event.event_id().map_or_else(
                || OwnedEventId::from_str("$dummy").unwrap(),
                std::borrow::ToOwned::to_owned,
            );

            let is_edited = event.content().as_message().is_some_and(Message::is_edited);

            let sender_profile = match event.sender_profile() {
                TimelineDetails::Ready(profile) => {
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
                        membership: matrix_sdk::ruma::events::room::member::MembershipState::Join,
                        presence: selvedge_shared::PresenceState::Unknown,
                        is_verified,
                    })
                }
                _ => None,
            };

            TimelineItem::Event(EventItem {
                event_id,
                sender: event.sender().to_owned(),
                sender_profile,
                timestamp: MilliSecondsSinceUnixEpoch(event.timestamp().0),
                content: Box::new(content),
                reactions: IndexMap::default(),
                read_receipts: Vec::default(),
                delivery_status,
                in_reply_to: None,
                reply_details: None,
                is_edited,
                latest_edit: None,
                thread_root_id: None,
                is_highlight: event.is_highlighted(),
                should_group: false,
                encryption_status: EncryptionStatus::Unencrypted,
            })
        }
        TimelineItemKind::Virtual(virt) => match virt {
            VirtualTimelineItem::DateDivider(ts) => {
                TimelineItem::Virtual(VirtualItem::DayDivider {
                    ts: MilliSecondsSinceUnixEpoch(ts.0),
                })
            }
            _ => TimelineItem::Virtual(VirtualItem::LoadingIndicator),
        },
    }
}

pub(crate) async fn map_timeline_diff(
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

#[allow(clippy::future_not_send)]
pub(crate) async fn room_list_item_to_view(
    client: Client,
    item: RoomListItem,
) -> RoomListEntryView {
    let unread = item.unread_notification_counts();

    let last_activity = item
        .latest_event()
        .and_then(|e| e.event().timestamp())
        .unwrap_or_else(|| MilliSecondsSinceUnixEpoch(0u32.into()));

    let is_encrypted = if let Some(room) = client.get_room(item.room_id()) {
        room.encryption_state().is_encrypted()
    } else {
        false
    };

    let summary = RoomSummary {
        room_id: item.room_id().to_owned(),
        name: item.name(),
        avatar_url: item.avatar_url(),
        notification_count: unread.notification_count,
        highlight_count: unread.highlight_count,
        unread_count: 0,
        is_direct: item.is_direct().await.unwrap_or(false),
        last_message_preview: None,
        last_activity,
        has_active_call: false,
        active_call_participant_count: 0,
        is_encrypted,
        tags: HashSet::new(),
    };

    RoomListEntryView::Filled(summary)
}

#[allow(clippy::future_not_send)]
pub(crate) async fn map_room_list_diff(
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
