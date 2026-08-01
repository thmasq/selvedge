use eyeball_im::VectorDiff;
use indexmap::IndexMap;
use matrix_sdk::Client;
use matrix_sdk::ruma::MilliSecondsSinceUnixEpoch;
use matrix_sdk::ruma::events::room::member::MembershipState;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk_ui::room_list_service::RoomListItem;
use matrix_sdk_ui::timeline::TimelineEventShieldState;
use matrix_sdk_ui::timeline::{
    EventSendState, TimelineDetails, TimelineItemContent, TimelineItemKind, VirtualTimelineItem,
};
use ruma::presence::PresenceState;
use selvedge_shared::{
    DeliveryStatus, EncryptionStatus, EventItem, ModelError, RoomListEntryDiff, RoomListEntryView,
    RoomSummary, TimelineContent, TimelineDiff, TimelineItem, VirtualItem,
};
use std::collections::HashSet;
use std::str::FromStr;
use std::sync::Arc;

pub async fn map_timeline_item_safe(
    client: &Client,
    item: &matrix_sdk_ui::timeline::TimelineItem,
) -> TimelineItem {
    match item.kind() {
        TimelineItemKind::Event(event) => {
            let mut content = match event.content() {
                TimelineItemContent::MsgLike(msg) => msg.as_message().map_or_else(
                    || TimelineContent::Unsupported,
                    |msg_content| {
                        let ruma_content =
                            RoomMessageEventContent::new(msg_content.msgtype().clone());
                        ruma_content.into()
                    },
                ),
                _ => TimelineContent::Unsupported,
            };

            if matches!(content, TimelineContent::Unsupported)
                && let Some(raw_json) = event.latest_json()
                && let Ok(Some(event_type)) = raw_json.get_field::<String>("type")
                && event_type == "m.room.encrypted"
            {
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

            let delivery_status = event
                .send_state()
                .map_or(DeliveryStatus::Synced, |local_echo| match local_echo {
                    EventSendState::NotSentYet { .. } => {
                        DeliveryStatus::Sending(matrix_sdk::ruma::OwnedTransactionId::from("dummy"))
                    }
                    EventSendState::Sent { .. } => DeliveryStatus::Sent,
                    EventSendState::SendingFailed { .. } => DeliveryStatus::Error(
                        ModelError::DeliveryFailed("Failed to send".to_string()),
                    ),
                });

            let event_id = event.event_id().map_or_else(
                || matrix_sdk::ruma::OwnedEventId::from_str("$dummy").unwrap(),
                std::borrow::ToOwned::to_owned,
            );

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
                        membership: MembershipState::Join,
                        presence: PresenceState::Unavailable,
                        is_verified,
                    })
                }
                _ => None,
            };

            let mut reactions = IndexMap::new();
            let mut in_reply_to = None;
            let mut reply_details = None;
            let mut thread_root_id = None;

            if let TimelineItemContent::MsgLike(msg_like) = event.content() {
                for (emoji, users) in msg_like.reactions.iter() {
                    let count = users.len() as u64;
                    let me_reacted = client
                        .user_id()
                        .map_or(false, |my_id| users.contains_key(my_id));

                    reactions.insert(
                        emoji.clone(),
                        selvedge_shared::model::ReactionDetails { count, me_reacted },
                    );
                }

                thread_root_id = msg_like.thread_root.clone();

                if let Some(reply_info) = &msg_like.in_reply_to {
                    in_reply_to = Some(reply_info.event_id.clone());

                    if let TimelineDetails::Ready(replied_event) = &reply_info.event {
                        let reply_content = Box::new(match &replied_event.content {
                            TimelineItemContent::MsgLike(rl_msg) => {
                                rl_msg
                                    .as_message()
                                    .map_or(TimelineContent::Unsupported, |m| {
                                        TimelineContent::from(RoomMessageEventContent::new(
                                            m.msgtype().clone(),
                                        ))
                                    })
                            }
                            _ => TimelineContent::Unsupported,
                        });

                        reply_details = Some(selvedge_shared::model::ReplyDetails {
                            sender: replied_event.sender.clone(),
                            sender_display_name: match &replied_event.sender_profile {
                                TimelineDetails::Ready(p) => p.display_name.clone(),
                                _ => None,
                            },
                            content: reply_content,
                        });
                    }
                }
            }

            let encryption_status = match event.get_shield(false) {
                TimelineEventShieldState::None => {
                    if event.encryption_info().is_some() {
                        EncryptionStatus::Verified
                    } else {
                        EncryptionStatus::Unencrypted
                    }
                }
                _ => EncryptionStatus::Unverified,
            };

            let is_edited = event.content().as_message().is_some_and(|m| m.is_edited());

            TimelineItem::Event(Box::new(EventItem {
                event_id,
                sender: event.sender().to_owned(),
                sender_profile,
                timestamp: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(event.timestamp().0),
                content: Box::new(content),
                reactions,
                read_receipts: event.read_receipts().keys().cloned().collect(),
                delivery_status,
                in_reply_to,
                reply_details,
                is_edited,
                latest_edit: None,
                thread_root_id,
                is_highlight: event.is_highlighted(),
                should_group: false,
                encryption_status,
            }))
        }
        TimelineItemKind::Virtual(virt) => match virt {
            VirtualTimelineItem::DateDivider(ts) => {
                TimelineItem::Virtual(VirtualItem::DayDivider {
                    ts: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(ts.0),
                })
            }
            _ => TimelineItem::Virtual(VirtualItem::LoadingIndicator),
        },
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

#[allow(clippy::future_not_send)]
pub async fn room_list_item_to_view(client: Client, item: RoomListItem) -> RoomListEntryView {
    let unread = item.unread_notification_counts();

    let last_activity = client
        .get_room(item.room_id())
        .and_then(|room| room.latest_event_timestamp())
        .unwrap_or_else(|| MilliSecondsSinceUnixEpoch(0u32.into()));

    let room = client.get_room(item.room_id());

    let receipts = item.read_receipts();

    let is_encrypted = room
        .as_ref()
        .is_some_and(|r| r.encryption_state().is_encrypted());

    let summary = RoomSummary {
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
    };

    RoomListEntryView::Filled(summary)
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
