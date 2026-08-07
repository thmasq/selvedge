use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk_ui::timeline::{EncryptedMessage, MsgLikeContent, MsgLikeKind};
use ruma::UserId;
use selvedge_shared::model::{MessageContent, PollState, TimelineContent};

pub fn map(msg: &MsgLikeContent, own_user_id: Option<&UserId>) -> TimelineContent {
    match &msg.kind {
        MsgLikeKind::Message(m) => {
            TimelineContent::from(RoomMessageEventContent::new(m.msgtype().clone()))
        }
        MsgLikeKind::Redacted => TimelineContent::Redacted,
        MsgLikeKind::Sticker(s) => {
            let source = match &s.content().source {
                matrix_sdk::ruma::events::sticker::StickerMediaSource::Plain(uri) => {
                    selvedge_shared::model::MediaSource::Plain(uri.clone())
                }
                matrix_sdk::ruma::events::sticker::StickerMediaSource::Encrypted(file) => {
                    selvedge_shared::model::MediaSource::Encrypted(file.clone())
                }
                _ => return TimelineContent::Unsupported,
            };

            TimelineContent::Message(MessageContent::Sticker {
                body: s.content().body.clone(),
                source,
                info: Some(s.content().info.clone().into()),
            })
        }
        MsgLikeKind::Poll(p) => {
            let results = p.results();

            let mut vote_counts: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            for answers in results.votes.values() {
                for answer_id in answers {
                    *vote_counts.entry(answer_id.clone()).or_insert(0) += 1;
                }
            }

            let answers = results
                .answers
                .into_iter()
                .map(|a| {
                    let is_selected = own_user_id.is_some_and(|uid| {
                        results
                            .votes
                            .get(uid.as_str())
                            .is_some_and(|votes| votes.contains(&a.id))
                    });

                    selvedge_shared::model::PollAnswer {
                        id: a.id.clone(),
                        text: a.text.clone(),
                        count: vote_counts.get(&a.id).copied().unwrap_or(0),
                        is_selected,
                    }
                })
                .collect();

            TimelineContent::Poll(PollState {
                question: results.question,
                answers,
                is_closed: results.end_time.is_some(),
            })
        }
        MsgLikeKind::UnableToDecrypt(utd) => match utd {
            EncryptedMessage::OlmV1Curve25519AesSha2 { sender_key } => {
                TimelineContent::Undecryptable {
                    session_id: String::new(),
                    sender_key: sender_key.clone(),
                }
            }
            EncryptedMessage::MegolmV1AesSha2 { session_id, .. } => {
                TimelineContent::Undecryptable {
                    session_id: session_id.clone(),
                    sender_key: String::new(),
                }
            }
            EncryptedMessage::Unknown => TimelineContent::Unsupported,
        },
        MsgLikeKind::LiveLocation(l) => TimelineContent::LiveLocation {
            is_active: l.is_live(),
        },
        MsgLikeKind::Other(o) => TimelineContent::OtherMessageLike {
            event_type: o.event_type().to_string(),
            body: None,
        },
    }
}
