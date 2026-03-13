use super::super::MatrixActor;
use super::super::mapping::{map_timeline_diff, map_timeline_item_safe};
use futures::StreamExt;
use matrix_sdk::ruma::events::AnyInitialStateEvent;
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk::ruma::serde::Raw;
use matrix_sdk::ruma::{EventEncryptionAlgorithm, OwnedRoomId};
use matrix_sdk_ui::timeline::RoomExt;
use selvedge_shared::{
    ActorError, RoomDetails, RoomPermissions, RoomTrustLevel, TimelineDiff, message::ToShell,
};
use std::collections::{HashMap, HashSet, VecDeque};
use wasm_bindgen_futures::spawn_local;

impl MatrixActor {
    #[allow(clippy::future_not_send)]
    pub(crate) async fn open_room(&self, room_id: OwnedRoomId) {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            if let Some(room) = client.get_room(&room_id) {
                let has_timeline = self.active_timelines.borrow().contains_key(&room_id);

                if !has_timeline {
                    if let Ok(timeline) = room.timeline_builder().build().await {
                        let is_encrypted = room.encryption_state().is_encrypted();

                        let trust_level = if is_encrypted {
                            RoomTrustLevel::Trusted
                        } else {
                            RoomTrustLevel::Plain
                        };

                        let members_map = HashMap::new();

                        self.send_event(ToShell::RoomDetailsUpdate {
                            room_id: room_id.clone(),
                            details: RoomDetails {
                                room_id: room_id.clone(),
                                name: room.name(),
                                topic: room.topic(),
                                avatar_url: room.avatar_url(),
                                members: members_map,
                                timeline: VecDeque::new(),
                                typing_users: HashSet::new(),
                                active_call: None,
                                is_encrypted,
                                trust_level,
                                permissions: RoomPermissions::default(),
                                prev_batch: None,
                                next_batch: None,
                                fully_read_marker: None,
                            },
                        });

                        let (items, mut stream) = timeline.subscribe().await;

                        let mut initial_views = Vec::new();
                        for i in items {
                            let mapped = map_timeline_item_safe(&client, &i).await;
                            self.search_index.borrow_mut().index_item(&room_id, &mapped);
                            initial_views.push(mapped);
                        }

                        self.send_event(ToShell::TimelineDiff {
                            room_id: room_id.clone(),
                            diff: vec![TimelineDiff::Reset {
                                entries: initial_views,
                            }],
                        });

                        self.active_timelines
                            .borrow_mut()
                            .insert(room_id.clone(), std::rc::Rc::new(timeline));

                        let sender = self.event_sender.clone();
                        let stream_room_id = room_id.clone();
                        let search_index = self.search_index.clone();
                        let mapper_client = client.clone();

                        spawn_local(async move {
                            while let Some(diffs) = stream.next().await {
                                let mut mapped_diffs = Vec::new();

                                for diff in diffs {
                                    let mapped_diff = map_timeline_diff(&mapper_client, diff).await;

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

                                    mapped_diffs.push(mapped_diff);
                                }

                                let _ = sender.unbounded_send(ToShell::TimelineDiff {
                                    room_id: stream_room_id.clone(),
                                    diff: mapped_diffs,
                                });
                            }
                        });
                    }
                }
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
