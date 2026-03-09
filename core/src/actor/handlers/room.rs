use super::super::MatrixActor;
use super::super::mapping::{map_timeline_diff, map_timeline_item_safe};
use super::super::message::ToShell;
use crate::model::{ActorError, RoomDetails, TimelineDiff, TimelineItem};
use futures::StreamExt;
use gloo_storage::{LocalStorage, Storage};
use matrix_sdk::ruma::events::AnyInitialStateEvent;
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{EventEncryptionAlgorithm, OwnedRoomId};
use matrix_sdk_ui::timeline::RoomExt;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

impl MatrixActor {
    #[allow(clippy::future_not_send)]
    pub(crate) async fn open_room(&self, room_id: OwnedRoomId) {
        let client = self.client.borrow().clone();
        if let Some(client) = client
            && let Some(room) = client.get_room(&room_id)
        {
            let has_timeline = self.active_timelines.borrow().contains_key(&room_id);

            if !has_timeline && let Ok(timeline) = room.timeline_builder().build().await {
                let is_encrypted = room.encryption_state().is_encrypted();

                let mut trust_level = if is_encrypted {
                    crate::model::RoomTrustLevel::Trusted
                } else {
                    crate::model::RoomTrustLevel::Plain
                };

                let mut members_map = HashMap::new();
                if let Ok(members) = room.members(matrix_sdk::RoomMemberships::ACTIVE).await {
                    for member in members {
                        let user_id = member.user_id().to_owned();

                        let is_verified = if is_encrypted {
                            let is_user_verified = client
                                .encryption()
                                .get_user_identity(&user_id)
                                .await
                                .ok()
                                .flatten()
                                .is_some_and(|identity| identity.is_verified());

                            let storage_key = format!("trust_state_{}", user_id);
                            let prev_verified: Option<bool> = LocalStorage::get(&storage_key).ok();

                            if !is_user_verified {
                                if prev_verified == Some(true) {
                                    trust_level = crate::model::RoomTrustLevel::Warning;
                                } else if trust_level == crate::model::RoomTrustLevel::Trusted {
                                    trust_level = crate::model::RoomTrustLevel::Normal;
                                }
                            } else if let Ok(devices) =
                                client.encryption().get_user_devices(&user_id).await
                            {
                                for device in devices.devices() {
                                    if !device.is_cross_signed_by_owner() {
                                        trust_level = crate::model::RoomTrustLevel::Warning;
                                        break;
                                    }
                                }
                            }

                            let _ = LocalStorage::set(&storage_key, is_user_verified);

                            is_user_verified
                        } else {
                            false
                        };

                        members_map.insert(
                            user_id.clone(),
                            crate::model::MemberProfile {
                                user_id,
                                display_name: member.display_name().map(ToOwned::to_owned),
                                avatar_url: member.avatar_url().map(ToOwned::to_owned),
                                membership: member.membership().clone(),
                                presence: crate::model::PresenceState::Unknown,
                                is_verified,
                            },
                        );
                    }
                }

                self.send_event(ToShell::RoomDetailsUpdate {
                    room_id: room_id.clone(),
                    details: RoomDetails {
                        room_id: room_id.clone(),
                        name: room.name(),
                        topic: room.topic(),
                        avatar_url: room.avatar_url(),
                        members: members_map,
                        timeline: std::collections::VecDeque::new(),
                        typing_users: HashSet::new(),
                        active_call: None,
                        is_encrypted,
                        trust_level,
                        permissions: crate::model::RoomPermissions::default(),
                        prev_batch: None,
                        next_batch: None,
                        fully_read_marker: None,
                    },
                });

                let (items, mut stream) = timeline.subscribe().await;

                let initial_views: Vec<TimelineItem> = items
                    .into_iter()
                    .map(|i| {
                        let mapped = map_timeline_item_safe(&i);
                        self.search_index.borrow_mut().index_item(&room_id, &mapped);
                        mapped
                    })
                    .collect();

                self.send_event(ToShell::TimelineDiff {
                    room_id: room_id.clone(),
                    diff: vec![TimelineDiff::Reset {
                        entries: initial_views,
                    }],
                });

                self.active_timelines
                    .borrow_mut()
                    .insert(room_id.clone(), Rc::new(timeline));

                let sender = self.event_sender.clone();
                let stream_room_id = room_id.clone();
                let search_index = self.search_index.clone();

                spawn_local(async move {
                    while let Some(diffs) = stream.next().await {
                        let mapped_diffs: Vec<TimelineDiff> = diffs
                            .into_iter()
                            .map(|diff| {
                                let mapped_diff = map_timeline_diff(diff);

                                match &mapped_diff {
                                    TimelineDiff::Append { entries }
                                    | TimelineDiff::Reset { entries } => {
                                        let mut idx = search_index.borrow_mut();
                                        for entry in entries {
                                            idx.index_item(&stream_room_id, entry);
                                        }
                                    }
                                    TimelineDiff::PushFront { entry }
                                    | TimelineDiff::PushBack { entry }
                                    | TimelineDiff::Insert { entry, .. }
                                    | TimelineDiff::Set { entry, .. } => {
                                        search_index
                                            .borrow_mut()
                                            .index_item(&stream_room_id, entry);
                                    }
                                    _ => {}
                                }

                                mapped_diff
                            })
                            .collect();

                        let _ = sender.unbounded_send(ToShell::TimelineDiff {
                            room_id: stream_room_id.clone(),
                            diff: mapped_diffs,
                        });
                    }
                });
            }
        }
    }

    pub(crate) fn close_room(&self, room_id: OwnedRoomId) {
        self.active_timelines.borrow_mut().remove(&room_id);
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn set_typing(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
        typing: bool,
    ) -> Vec<ToShell> {
        let room = self
            .client
            .borrow()
            .as_ref()
            .and_then(|c| c.get_room(&room_id));
        if let Some(room) = room {
            match room.typing_notice(typing).await {
                Ok(_) => vec![ToShell::CommandResult {
                    request_id,
                    success: true,
                    error: None,
                }],
                Err(e) => vec![ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(e.to_string())),
                }],
            }
        } else {
            vec![ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            }]
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn join_room(&self, request_id: String, room_id: OwnedRoomId) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            match client.join_room_by_id(&room_id).await {
                Ok(_) => vec![ToShell::CommandResult {
                    request_id,
                    success: true,
                    error: None,
                }],
                Err(e) => vec![ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(e.to_string())),
                }],
            }
        } else {
            vec![ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            }]
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn leave_room(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
    ) -> Vec<ToShell> {
        let room = self
            .client
            .borrow()
            .as_ref()
            .and_then(|c| c.get_room(&room_id));
        if let Some(room) = room {
            match room.leave().await {
                Ok(_) => {
                    self.active_timelines.borrow_mut().remove(&room_id);
                    vec![ToShell::CommandResult {
                        request_id,
                        success: true,
                        error: None,
                    }]
                }
                Err(e) => vec![ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(e.to_string())),
                }],
            }
        } else {
            vec![ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            }]
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn create_room(
        &self,
        request_id: String,
        name: String,
        topic: Option<String>,
        is_encrypted: bool,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            let mut request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
            request.name = Some(name);
            request.topic = topic;

            if is_encrypted {
                let content =
                    RoomEncryptionEventContent::new(EventEncryptionAlgorithm::MegolmV1AesSha2);

                let raw_event = serde_json::json!({
                    "type": "m.room.encryption",
                    "state_key": "",
                    "content": content
                });

                if let Ok(raw_initial_state) =
                    serde_json::from_value::<Raw<AnyInitialStateEvent>>(raw_event)
                {
                    request.initial_state.push(raw_initial_state);
                }
            }

            match client.create_room(request).await {
                Ok(_) => vec![ToShell::CommandResult {
                    request_id,
                    success: true,
                    error: None,
                }],
                Err(e) => vec![ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(e.to_string())),
                }],
            }
        } else {
            vec![ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            }]
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn load_history(&self, room_id: OwnedRoomId) -> Vec<ToShell> {
        let timeline = self.active_timelines.borrow().get(&room_id).cloned();
        if let Some(timeline) = timeline
            && let Err(e) = timeline.paginate_backwards(20).await
        {
            return vec![ToShell::BackgroundError(ActorError::PaginationFailed(
                e.to_string(),
            ))];
        }
        vec![]
    }
}
