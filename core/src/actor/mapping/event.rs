use indexmap::IndexMap;
use matrix_sdk::Client;
use matrix_sdk::ruma::OwnedEventId;
use matrix_sdk::ruma::events::room::member::MembershipState;
use matrix_sdk_ui::timeline::{
    EmbeddedEvent, EventSendState, EventTimelineItem, MsgLikeContent, TimelineDetails,
    TimelineEventShieldState, TimelineItemContent,
};
use ruma::presence::PresenceState;
use selvedge_shared::{DeliveryStatus, EncryptionStatus, EventItem, ModelError};
use std::str::FromStr;

use super::content::resolve_content;

pub fn compute_delivery_status(event: &EventTimelineItem) -> DeliveryStatus {
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

pub fn resolve_event_id(event: &EventTimelineItem) -> matrix_sdk::ruma::OwnedEventId {
    event.event_id().map_or_else(
        || matrix_sdk::ruma::OwnedEventId::from_str("$dummy").unwrap(),
        std::borrow::ToOwned::to_owned,
    )
}

pub async fn build_sender_profile(
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

pub fn extract_reactions(
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

pub fn build_reply_details(
    client: &Client,
    replied_event: &EmbeddedEvent,
) -> selvedge_shared::model::ReplyDetails {
    let reply_content = Box::new(resolve_content(&replied_event.content, client.user_id()));

    selvedge_shared::model::ReplyDetails {
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
    pub reply_details: Option<selvedge_shared::model::ReplyDetails>,
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
    let content = resolve_content(event.content(), client.user_id());
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
