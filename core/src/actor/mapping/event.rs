use super::content::resolve_content;

use indexmap::IndexMap;
use matrix_sdk::ruma::events::room::member::MembershipState;
use matrix_sdk::ruma::{MilliSecondsSinceUnixEpoch, OwnedEventId};
use matrix_sdk::{Client, ruma::OwnedTransactionId};
use matrix_sdk_ui::timeline::{
    EmbeddedEvent, EventSendState, EventTimelineItem, Message, MsgLikeContent, MsgLikeKind,
    ReactionStatus, TimelineDetails, TimelineEventShieldState, TimelineItemContent,
};
use ruma::presence::PresenceState;
use selvedge_shared::{
    DeliveryStatus, EncryptionStatus, EventItem, MemberProfile, ModelError, TimelineContent,
    model::{ReactionDetails, ReplyDetails},
};
use std::str::FromStr;

pub fn compute_delivery_status(event: &EventTimelineItem) -> DeliveryStatus {
    event
        .send_state()
        .map_or(DeliveryStatus::Synced, |local_echo| match local_echo {
            EventSendState::NotSentYet { progress } => {
                let txn_id = event.transaction_id().map_or_else(
                    || OwnedTransactionId::from("unknown"),
                    std::borrow::ToOwned::to_owned,
                );

                let progress_pct = progress.as_ref().and_then(|p| {
                    let total = p.progress.total as f32;
                    if total > 0.0 {
                        Some(p.progress.current as f32 / total)
                    } else {
                        None
                    }
                });

                DeliveryStatus::Sending {
                    txn_id,
                    progress_pct,
                }
            }
            EventSendState::Sent { .. } => DeliveryStatus::Sent,
            EventSendState::SendingFailed { .. } => {
                DeliveryStatus::Error(ModelError::DeliveryFailed("Failed to send".to_string()))
            }
        })
}

pub fn resolve_event_id(event: &EventTimelineItem) -> matrix_sdk::ruma::OwnedEventId {
    event.event_id().map_or_else(
        || matrix_sdk::ruma::OwnedEventId::from_str("$dummy").unwrap(),
        std::borrow::ToOwned::to_owned,
    )
}

pub async fn build_sender_profile(
    client: &Client,
    event: &EventTimelineItem,
) -> Option<MemberProfile> {
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

    Some(MemberProfile {
        user_id: event.sender().to_owned(),
        display_name: profile.display_name.clone(),
        avatar_url: profile.avatar_url.clone(),
        membership: MembershipState::Join,
        presence: PresenceState::Unavailable,
        is_verified,
    })
}

pub fn extract_reactions(
    client: &Client,
    msg_like: &MsgLikeContent,
) -> IndexMap<String, ReactionDetails> {
    let mut reactions = IndexMap::new();
    let my_user_id = client.user_id();

    for (emoji, users) in msg_like.reactions.iter() {
        let count = users.len() as u64;
        let me_reacted = my_user_id.is_some_and(|my_id| users.contains_key(my_id));

        let mut my_reaction_event_id = None;
        if let Some(my_id) = my_user_id {
            if let Some(reaction_info) = users.get(my_id) {
                if let ReactionStatus::RemoteToRemote(event_id) = &reaction_info.status {
                    my_reaction_event_id = Some(event_id.clone());
                }
            }
        }

        reactions.insert(
            emoji.clone(),
            ReactionDetails {
                count,
                me_reacted,
                my_reaction_event_id,
            },
        );
    }
    reactions
}

pub fn build_reply_details(client: &Client, replied_event: &EmbeddedEvent) -> ReplyDetails {
    let reply_content = Box::new(resolve_content(&replied_event.content, client.user_id()));

    ReplyDetails {
        sender: replied_event.sender.clone(),
        sender_display_name: match &replied_event.sender_profile {
            TimelineDetails::Ready(p) => p.display_name.clone(),
            _ => None,
        },
        content: reply_content,
    }
}

pub struct ReplyInfo {
    pub in_reply_to: Option<OwnedEventId>,
    pub reply_details: Option<ReplyDetails>,
    pub thread_root_id: Option<OwnedEventId>,
}

pub fn extract_reply_info(client: &Client, msg_like: &MsgLikeContent) -> ReplyInfo {
    let thread_root_id = msg_like.thread_root.clone();

    let Some(reply_info) = &msg_like.in_reply_to else {
        return ReplyInfo {
            in_reply_to: None,
            reply_details: None,
            thread_root_id,
        };
    };

    let reply_details = match &reply_info.event {
        TimelineDetails::Ready(replied_event) => Some(build_reply_details(client, replied_event)),
        _ => None,
    };

    ReplyInfo {
        in_reply_to: Some(reply_info.event_id.clone()),
        reply_details,
        thread_root_id,
    }
}

pub fn compute_encryption_status(event: &EventTimelineItem) -> EncryptionStatus {
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

pub async fn map_event_item(client: &Client, event: &EventTimelineItem) -> EventItem {
    let mut content = resolve_content(event.content(), client.user_id());

    if let TimelineContent::OtherMessageLike {
        event_type: _,
        body,
    } = &mut content
    {
        if let Some(raw) = event.original_json() {
            *body = raw.get_field::<String>("body").ok().flatten();
        }
    }

    let delivery_status = compute_delivery_status(event);
    let event_id = resolve_event_id(event);
    let sender_profile = build_sender_profile(client, event).await;

    let (reactions, reply_info) = match event.content() {
        TimelineItemContent::MsgLike(msg_like) => (
            extract_reactions(client, msg_like),
            extract_reply_info(client, msg_like),
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
    let is_trusted = matches!(
        encryption_status,
        EncryptionStatus::Verified | EncryptionStatus::Unencrypted
    );

    let is_edited = event.content().as_message().is_some_and(Message::is_edited);

    let is_own_mention = match event.content() {
        TimelineItemContent::MsgLike(msg) => {
            if let MsgLikeKind::Message(m) = &msg.kind {
                if let Some(mentions) = m.mentions() {
                    let my_user_id = client.user_id();
                    my_user_id.is_some_and(|id| mentions.user_ids.contains(id)) || mentions.room
                } else {
                    false
                }
            } else {
                false
            }
        }
        _ => false,
    };

    EventItem {
        event_id,
        sender: event.sender().to_owned(),
        sender_profile,
        timestamp: MilliSecondsSinceUnixEpoch(event.timestamp().0),
        content: Box::new(content),
        reactions,
        read_receipts: event.read_receipts().keys().cloned().collect(),
        delivery_status,
        in_reply_to: reply_info.in_reply_to,
        reply_details: reply_info.reply_details,
        is_edited,
        latest_edit: None,
        thread_root_id: reply_info.thread_root_id,
        is_own_mention,
        is_highlight: event.is_highlighted(),
        is_trusted,
        should_group: false,
        encryption_status,
    }
}
