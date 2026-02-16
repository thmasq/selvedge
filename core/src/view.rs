use crate::model::{Message, MessageEvent, MessageTimeStamp, RoomInfo, StickerPack};
use matrix_sdk::ruma::{
    OwnedEventId, OwnedRoomId, UserId,
    events::{receipt::ReceiptThread, room::message::MessageType},
};
use selvedge_shared::{
    MessageStatus, MessageView, ReactionView, RoomView, StickerPackView, StickerView,
};

pub fn to_room_view(id: &OwnedRoomId, info: &RoomInfo, _current_user: &UserId) -> RoomView {
    let typing_users = info
        .users_typing
        .as_ref()
        .map(|(_, users)| {
            users
                .iter()
                .map(|u| {
                    info.display_names
                        .get(u)
                        .cloned()
                        .unwrap_or_else(|| u.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    RoomView {
        id: id.to_string(),
        name: info
            .name
            .clone()
            .unwrap_or_else(|| "Empty Room".to_string()),
        topic: info.topic.clone(),
        avatar_url: info.avatar_url.clone(),
        typing_users,
        unread_count: info.notification_count,
        is_direct: info.is_direct,
        is_encrypted: info.is_encrypted,
        members_count: info.active_members_count,
    }
}

pub fn to_message_view(msg: &Message, info: &RoomInfo, current_user: &UserId) -> MessageView {
    let (body, html, reply_to, is_edited) = extract_content(&msg.event);

    let timestamp: u64 = match msg.timestamp {
        MessageTimeStamp::OriginServer(ts) => ts.0.into(),
        MessageTimeStamp::LocalEcho => 0u64,
    };

    let status = if !msg.downloaded {
        MessageStatus::Local
    } else {
        MessageStatus::Synced
    };

    let event_id = msg.event.event_id();

    let mut reactions = Vec::new();
    if let Some(msg_reactions) = info.reactions.get(event_id) {
        for (key, users) in msg_reactions {
            reactions.push(ReactionView {
                key: key.clone(),
                count: users.len(),
                includes_me: users.contains(current_user),
            });
        }
    }

    let mut read_receipts = Vec::new();
    if let Some(main_receipts) = info.event_receipts.get(&ReceiptThread::Main) {
        if let Some(users) = main_receipts.get(event_id) {
            read_receipts = users.iter().map(|u| u.to_string()).collect();
        }
    }

    MessageView {
        id: event_id.to_string(),
        sender: msg.sender.to_string(),
        sender_name: info.display_names.get(&msg.sender).cloned(),
        sender_avatar_url: info.user_avatars.get(&msg.sender).cloned(),
        body,
        html_content: html.or(msg.formatted_body.clone()),
        timestamp,
        is_mine: msg.sender == current_user,
        status,
        reply_to_id: reply_to.map(|id| id.to_string()),
        is_edited,
        reactions,
        read_receipts,
    }
}

pub fn to_sticker_pack_view(pack: &StickerPack) -> StickerPackView {
    StickerPackView {
        name: pack.name.clone(),
        avatar_url: pack.avatar_url.clone(),
        stickers: pack
            .images
            .iter()
            .map(|(code, url)| StickerView {
                shortcode: code.clone(),
                url: url.clone(),
                body: code.clone(),
            })
            .collect(),
    }
}

fn extract_content(event: &MessageEvent) -> (String, Option<String>, Option<OwnedEventId>, bool) {
    match event {
        MessageEvent::Original(ev) => {
            if let Some(original_ev) = ev.as_original() {
                let (body, html) = match &original_ev.content.msgtype {
                    MessageType::Text(t) => {
                        (t.body.clone(), t.formatted.as_ref().map(|f| f.body.clone()))
                    }
                    MessageType::Emote(e) => (
                        format!("* {}", e.body),
                        e.formatted.as_ref().map(|f| f.body.clone()),
                    ),
                    MessageType::Notice(n) => {
                        (n.body.clone(), n.formatted.as_ref().map(|f| f.body.clone()))
                    }
                    MessageType::Image(i) => (i.body.clone(), None),
                    MessageType::File(f) => (f.body.clone(), None),
                    MessageType::Video(v) => (v.body.clone(), None),
                    MessageType::Audio(a) => (a.body.clone(), None),
                    _ => ("Unsupported message type".to_string(), None),
                };

                let reply_to = original_ev
                    .content
                    .relates_to
                    .as_ref()
                    .and_then(|r| match r {
                        matrix_sdk::ruma::events::room::message::Relation::Reply {
                            in_reply_to,
                        } => Some(in_reply_to.event_id.clone()),
                        _ => None,
                    });

                let is_edited = original_ev.content.relates_to.as_ref().map_or(false, |r| {
                    matches!(
                        r,
                        matrix_sdk::ruma::events::room::message::Relation::Replacement(_)
                    )
                });

                (body, html, reply_to, is_edited)
            } else {
                ("[Redacted]".to_string(), None, None, false)
            }
        }
        MessageEvent::Local(_, content) => {
            let body = match &content.msgtype {
                MessageType::Text(t) => t.body.clone(),
                _ => "Sending...".to_string(),
            };
            (body, None, None, false)
        }
        MessageEvent::Redacted(_) => ("[Redacted]".to_string(), None, None, false),
        MessageEvent::EncryptedOriginal(_) | MessageEvent::EncryptedRedacted(_) => {
            ("**Encrypted**".to_string(), None, None, false)
        }
        _ => ("".to_string(), None, None, false),
    }
}
