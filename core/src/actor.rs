use crate::model::{
    ActorError, DeliveryStatus, EventItem, ModelError, RoomDetails, RoomListEntryDiff,
    RoomListEntryView, RoomSummary, TimelineContent, TimelineDiff, TimelineItem,
};
use eyeball_im::VectorDiff;
use futures::{StreamExt, channel::mpsc};
use gloo_worker::HandlerId;
use gloo_worker::{Worker, WorkerScope};
use indexmap::IndexMap;
use matrix_sdk::{
    Client,
    attachment::AttachmentConfig,
    ruma::{
        OwnedEventId, OwnedRoomId, OwnedTransactionId,
        events::room::message::RoomMessageEventContent,
    },
};
use matrix_sdk_ui::timeline::RoomExt;
use matrix_sdk_ui::{
    room_list_service::{RoomListItem, RoomListService, filters::new_filter_all},
    timeline::{EventSendState, Timeline, VirtualTimelineItem},
};
use ruma::MilliSecondsSinceUnixEpoch;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Serialize, Deserialize)]
pub enum ToActor {
    Login {
        homeserver_url: String,
        username: String,
        password: String,
    },
    RestoreSession,
    StartSync,
    OpenRoom {
        room_id: OwnedRoomId,
    },
    CloseRoom {
        room_id: OwnedRoomId,
    },
    SendMessage {
        request_id: String,
        room_id: OwnedRoomId,
        body: String,
        reply_to: Option<OwnedEventId>,
    },
    SendMedia {
        request_id: String,
        room_id: OwnedRoomId,
        filename: String,
        mime_type: String,
        data: Vec<u8>,
    },
    CreateRoom {
        request_id: String,
        name: String,
        topic: Option<String>,
        is_encrypted: bool,
    },
    JoinRoom {
        request_id: String,
        room_id: OwnedRoomId,
    },
    LeaveRoom {
        request_id: String,
        room_id: OwnedRoomId,
    },
    SetTyping {
        request_id: String,
        room_id: OwnedRoomId,
        typing: bool,
    },
    LoadHistory {
        room_id: OwnedRoomId,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub enum ToShell {
    LoginSuccess,
    LoginFailure(ActorError),
    BackgroundError(ActorError),
    RoomListDiff(Vec<RoomListEntryDiff>),
    RoomDetailsUpdate {
        room_id: OwnedRoomId,
        details: RoomDetails,
    },
    TimelineDiff {
        room_id: OwnedRoomId,
        diff: Vec<TimelineDiff>,
    },
    CommandResult {
        request_id: String,
        success: bool,
        error: Option<ActorError>,
    },
}

pub struct MatrixWorker {
    actor: Rc<MatrixActor>,
    bridge_id: Rc<RefCell<Option<HandlerId>>>,
}

impl Worker for MatrixWorker {
    type Input = ToActor;
    type Output = ToShell;
    type Message = ();

    fn create(scope: &WorkerScope<Self>) -> Self {
        let (tx, mut rx) = mpsc::unbounded();
        let actor = Rc::new(MatrixActor::new(tx));
        let bridge_id = Rc::new(RefCell::new(None));

        let scope_clone = scope.clone();
        let bridge_id_clone = bridge_id.clone();

        spawn_local(async move {
            while let Some(event) = rx.next().await {
                if let Some(id) = *bridge_id_clone.borrow() {
                    scope_clone.respond(id, event);
                }
            }
        });

        Self { actor, bridge_id }
    }

    fn update(&mut self, _scope: &WorkerScope<Self>, _msg: Self::Message) {}

    fn received(&mut self, scope: &WorkerScope<Self>, msg: Self::Input, id: HandlerId) {
        *self.bridge_id.borrow_mut() = Some(id);

        let actor = self.actor.clone();
        let scope = scope.clone();

        spawn_local(async move {
            let responses = actor.handle_message(msg).await;

            for response in responses {
                scope.respond(id, response);
            }
        });
    }
}

struct MatrixActor {
    client: RefCell<Option<Client>>,
    event_sender: mpsc::UnboundedSender<ToShell>,
    active_timelines: RefCell<HashMap<OwnedRoomId, Rc<Timeline>>>,
}

impl MatrixActor {
    fn new(event_sender: mpsc::UnboundedSender<ToShell>) -> Self {
        Self {
            client: RefCell::new(None),
            event_sender,
            active_timelines: RefCell::new(HashMap::new()),
        }
    }

    fn send_event(&self, event: ToShell) {
        let _ = self.event_sender.unbounded_send(event);
    }

    #[allow(clippy::future_not_send)]
    async fn handle_message(&self, msg: ToActor) -> Vec<ToShell> {
        match msg {
            ToActor::Login {
                homeserver_url,
                username,
                password,
            } => self.login(homeserver_url, username, password).await,
            ToActor::RestoreSession => self.restore_session().await,
            ToActor::StartSync => {
                self.start_sync();
                vec![]
            }
            ToActor::OpenRoom { room_id } => {
                self.open_room(room_id).await;
                vec![]
            }
            ToActor::CloseRoom { room_id } => {
                self.active_timelines.borrow_mut().remove(&room_id);
                vec![]
            }
            ToActor::SendMessage {
                request_id,
                room_id,
                body,
                reply_to: _, // TODO: handle replies
            } => self.send_message(request_id, room_id, body).await,
            ToActor::SendMedia {
                request_id,
                room_id,
                filename,
                mime_type,
                data,
            } => {
                self.send_media(request_id, room_id, filename, mime_type, data)
                    .await
            }
            ToActor::SetTyping {
                request_id,
                room_id,
                typing,
            } => {
                let room = self
                    .client
                    .borrow()
                    .as_ref()
                    .and_then(|c| c.get_room(&room_id));
                let response = if let Some(room) = room {
                    match room.typing_notice(typing).await {
                        Ok(_) => ToShell::CommandResult {
                            request_id,
                            success: true,
                            error: None,
                        },
                        Err(e) => ToShell::CommandResult {
                            request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(e.to_string())),
                        },
                    }
                } else {
                    ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::ClientNotInitialized),
                    }
                };
                vec![response]
            }
            ToActor::JoinRoom {
                request_id,
                room_id,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    match client.join_room_by_id(&room_id).await {
                        Ok(_) => ToShell::CommandResult {
                            request_id,
                            success: true,
                            error: None,
                        },
                        Err(e) => ToShell::CommandResult {
                            request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(e.to_string())),
                        },
                    }
                } else {
                    ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::ClientNotInitialized),
                    }
                };
                vec![response]
            }
            ToActor::LeaveRoom {
                request_id,
                room_id,
            } => {
                let room = self
                    .client
                    .borrow()
                    .as_ref()
                    .and_then(|c| c.get_room(&room_id));
                let response = if let Some(room) = room {
                    match room.leave().await {
                        Ok(_) => {
                            self.active_timelines.borrow_mut().remove(&room_id);
                            ToShell::CommandResult {
                                request_id,
                                success: true,
                                error: None,
                            }
                        }
                        Err(e) => ToShell::CommandResult {
                            request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(e.to_string())),
                        },
                    }
                } else {
                    ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::ClientNotInitialized),
                    }
                };
                vec![response]
            }
            ToActor::CreateRoom {
                request_id,
                name,
                topic,
                is_encrypted: _,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    let mut request =
                        matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
                    request.name = Some(name);
                    request.topic = topic;

                    // TODO: Add an m.room.encryption state event to `request.initial_state`
                    match client.create_room(request).await {
                        Ok(_) => ToShell::CommandResult {
                            request_id,
                            success: true,
                            error: None,
                        },
                        Err(e) => ToShell::CommandResult {
                            request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(e.to_string())),
                        },
                    }
                } else {
                    ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::ClientNotInitialized),
                    }
                };
                vec![response]
            }
            ToActor::LoadHistory { room_id } => {
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
    }

    #[allow(clippy::future_not_send)]
    async fn login(&self, url: String, user: String, pass: String) -> Vec<ToShell> {
        let client_builder = Client::builder()
            .homeserver_url(&url)
            .indexeddb_store("selvedge-store", None);

        match client_builder.build().await {
            Ok(client) => match client.matrix_auth().login_username(&user, &pass).await {
                Ok(_) => {
                    *self.client.borrow_mut() = Some(client);
                    vec![ToShell::LoginSuccess]
                }
                Err(e) => vec![ToShell::LoginFailure(ActorError::LoginFailed(
                    e.to_string(),
                ))],
            },
            Err(e) => vec![ToShell::LoginFailure(ActorError::LoginFailed(
                e.to_string(),
            ))],
        }
    }

    #[allow(clippy::future_not_send)]
    async fn restore_session(&self) -> Vec<ToShell> {
        let client_builder = Client::builder().indexeddb_store("selvedge-store", None);

        match client_builder.build().await {
            Ok(client) => {
                if client.session_meta().is_some() {
                    *self.client.borrow_mut() = Some(client);
                    vec![ToShell::LoginSuccess]
                } else {
                    vec![ToShell::LoginFailure(ActorError::LoginFailed(
                        "No saved session found".to_string(),
                    ))]
                }
            }
            Err(e) => vec![ToShell::LoginFailure(ActorError::LoginFailed(
                e.to_string(),
            ))],
        }
    }

    fn start_sync(&self) {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            let sender = self.event_sender.clone();

            spawn_local(async move {
                match RoomListService::new(client.clone()).await {
                    Ok(room_list_service) => {
                        let room_list_service = Rc::new(room_list_service);

                        {
                            let svc = room_list_service.clone();
                            spawn_local(async move {
                                let sync_stream = svc.sync();
                                futures::pin_mut!(sync_stream);
                                while sync_stream.next().await.is_some() {}
                            });
                        }

                        {
                            let svc = room_list_service;
                            let sender = sender.clone();

                            spawn_local(async move {
                                if let Ok(all_rooms) = svc.all_rooms().await {
                                    let (entries_stream, controller) =
                                        all_rooms.entries_with_dynamic_adapters(50);

                                    controller.set_filter(Box::new(new_filter_all(vec![])));

                                    futures::pin_mut!(entries_stream);

                                    while let Some(diffs) = entries_stream.next().await {
                                        let mapped = futures::future::join_all(
                                            diffs.into_iter().map(map_room_list_diff),
                                        )
                                        .await;

                                        let _ =
                                            sender.unbounded_send(ToShell::RoomListDiff(mapped));
                                    }
                                }
                            });
                        }
                    }
                    Err(e) => {
                        let _ = sender.unbounded_send(ToShell::BackgroundError(
                            ActorError::SyncInitializationFailed(e.to_string()),
                        ));
                    }
                }
            });
        }
    }

    #[allow(clippy::future_not_send)]
    async fn open_room(&self, room_id: OwnedRoomId) {
        let client = self.client.borrow().clone();
        if let Some(client) = client
            && let Some(room) = client.get_room(&room_id)
        {
            let has_timeline = self.active_timelines.borrow().contains_key(&room_id);

            if !has_timeline && let Ok(timeline) = room.timeline_builder().build().await {
                let (items, mut stream) = timeline.subscribe().await;

                let initial_views: Vec<TimelineItem> = items
                    .into_iter()
                    .map(|i| map_timeline_item_safe(&i))
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

                spawn_local(async move {
                    while let Some(diffs) = stream.next().await {
                        let mapped_diffs: Vec<TimelineDiff> =
                            diffs.into_iter().map(map_timeline_diff).collect();

                        let _ = sender.unbounded_send(ToShell::TimelineDiff {
                            room_id: stream_room_id.clone(),
                            diff: mapped_diffs,
                        });
                    }
                });
            }
        }
    }

    #[allow(clippy::future_not_send)]
    async fn send_message(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
        body: String,
    ) -> Vec<ToShell> {
        let timeline = self.active_timelines.borrow().get(&room_id).cloned();

        let result = if let Some(timeline) = timeline {
            let content = RoomMessageEventContent::text_plain(body);
            timeline
                .send(content.into())
                .await
                .map(|_| ())
                .map_err(|_e| ActorError::RoomOperationFailed("Timeline send failed".to_string()))
        } else {
            let room = self
                .client
                .borrow()
                .as_ref()
                .and_then(|c| c.get_room(&room_id));

            if let Some(room) = room {
                let content = RoomMessageEventContent::text_plain(body);
                room.send(content)
                    .await
                    .map(|_| ())
                    .map_err(|e| ActorError::RoomOperationFailed(e.to_string()))
            } else {
                Err(ActorError::ClientNotInitialized)
            }
        };

        let response = match result {
            Ok(_) => ToShell::CommandResult {
                request_id,
                success: true,
                error: None,
            },
            Err(e) => ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(e),
            },
        };

        vec![response]
    }

    #[allow(clippy::future_not_send)]
    async fn send_media(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
        filename: String,
        mime_type: String,
        data: Vec<u8>,
    ) -> Vec<ToShell> {
        let room = self
            .client
            .borrow()
            .as_ref()
            .and_then(|c| c.get_room(&room_id));

        let result = if let Some(room) = room {
            let config = AttachmentConfig::new();
            if let Ok(mime) = mime::Mime::from_str(&mime_type) {
                room.send_attachment(&filename, &mime, data, config)
                    .await
                    .map(|_| ())
                    .map_err(|e| ActorError::RoomOperationFailed(e.to_string()))
            } else {
                Err(ActorError::RoomOperationFailed(
                    "Invalid MIME type".to_string(),
                ))
            }
        } else {
            Err(ActorError::ClientNotInitialized)
        };

        let response = match result {
            Ok(_) => ToShell::CommandResult {
                request_id,
                success: true,
                error: None,
            },
            Err(e) => ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(e),
            },
        };

        vec![response]
    }
}

fn map_timeline_item_safe(item: &matrix_sdk_ui::timeline::TimelineItem) -> TimelineItem {
    match item.kind() {
        matrix_sdk_ui::timeline::TimelineItemKind::Event(event) => {
            let content = event.content().as_message().map_or_else(
                || TimelineContent::Unsupported,
                |msg| {
                    let ruma_content = RoomMessageEventContent::new(msg.msgtype().clone());
                    ruma_content.into()
                },
            );

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

            let is_edited = event
                .content()
                .as_message()
                .is_some_and(matrix_sdk_ui::timeline::Message::is_edited);

            TimelineItem::Event(EventItem {
                event_id,
                sender: event.sender().to_owned(),
                sender_profile: None,
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
                encryption_status: crate::model::EncryptionStatus::Unencrypted,
            })
        }
        matrix_sdk_ui::timeline::TimelineItemKind::Virtual(virt) => match virt {
            VirtualTimelineItem::DateDivider(ts) => {
                TimelineItem::Virtual(crate::model::VirtualItem::DayDivider {
                    ts: MilliSecondsSinceUnixEpoch(ts.0),
                })
            }
            _ => TimelineItem::Virtual(crate::model::VirtualItem::LoadingIndicator),
        },
    }
}

fn map_timeline_diff(diff: VectorDiff<Arc<matrix_sdk_ui::timeline::TimelineItem>>) -> TimelineDiff {
    match diff {
        VectorDiff::Append { values } => TimelineDiff::Append {
            entries: values
                .into_iter()
                .map(|v| map_timeline_item_safe(&v))
                .collect(),
        },
        VectorDiff::Clear => TimelineDiff::Clear,
        VectorDiff::PushFront { value } => TimelineDiff::PushFront {
            entry: map_timeline_item_safe(&value),
        },
        VectorDiff::PushBack { value } => TimelineDiff::PushBack {
            entry: map_timeline_item_safe(&value),
        },
        VectorDiff::PopFront => TimelineDiff::PopFront,
        VectorDiff::PopBack => TimelineDiff::PopBack,
        VectorDiff::Insert { index, value } => TimelineDiff::Insert {
            index,
            entry: map_timeline_item_safe(&value),
        },
        VectorDiff::Set { index, value } => TimelineDiff::Set {
            index,
            entry: map_timeline_item_safe(&value),
        },
        VectorDiff::Remove { index } => TimelineDiff::Remove { index },
        VectorDiff::Truncate { length } => TimelineDiff::Truncate { length },
        VectorDiff::Reset { values } => TimelineDiff::Reset {
            entries: values
                .into_iter()
                .map(|v| map_timeline_item_safe(&v))
                .collect(),
        },
    }
}

#[allow(clippy::future_not_send)]
async fn room_list_item_to_view(item: RoomListItem) -> RoomListEntryView {
    let unread = item.unread_notification_counts();

    let last_activity = item
        .latest_event()
        .and_then(|e| e.event().timestamp())
        .unwrap_or_else(|| MilliSecondsSinceUnixEpoch(0u32.into()));

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
        tags: HashSet::new(),
    };

    RoomListEntryView::Filled(summary)
}

#[allow(clippy::future_not_send)]
async fn map_room_list_diff(diff: VectorDiff<RoomListItem>) -> RoomListEntryDiff {
    match diff {
        VectorDiff::Append { values } => RoomListEntryDiff::Append {
            entries: futures::future::join_all(values.into_iter().map(room_list_item_to_view))
                .await,
        },
        VectorDiff::Clear => RoomListEntryDiff::Clear,
        VectorDiff::PushFront { value } => RoomListEntryDiff::PushFront {
            entry: room_list_item_to_view(value).await,
        },
        VectorDiff::PushBack { value } => RoomListEntryDiff::PushBack {
            entry: room_list_item_to_view(value).await,
        },
        VectorDiff::PopFront => RoomListEntryDiff::PopFront,
        VectorDiff::PopBack => RoomListEntryDiff::PopBack,
        VectorDiff::Insert { index, value } => RoomListEntryDiff::Insert {
            index,
            entry: room_list_item_to_view(value).await,
        },
        VectorDiff::Set { index, value } => RoomListEntryDiff::Set {
            index,
            entry: room_list_item_to_view(value).await,
        },
        VectorDiff::Remove { index } => RoomListEntryDiff::Remove { index },
        VectorDiff::Truncate { length } => RoomListEntryDiff::Truncate { length },
        VectorDiff::Reset { values } => RoomListEntryDiff::Reset {
            entries: futures::future::join_all(values.into_iter().map(room_list_item_to_view))
                .await,
        },
    }
}
