use crate::model::{
    DeliveryStatus, EventItem, RoomDetails, RoomListEntryDiff, RoomListEntryView, RoomSummary,
    TimelineContent, TimelineDiff, TimelineItem,
};
use eyeball_im::VectorDiff;
use futures::{StreamExt, channel::mpsc};
use gloo_worker::HandlerId;
use gloo_worker::{Worker, WorkerScope};
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
        room_id: OwnedRoomId,
        body: String,
        reply_to: Option<OwnedEventId>,
    },
    SendMedia {
        room_id: OwnedRoomId,
        filename: String,
        mime_type: String,
        data: Vec<u8>,
    },
    CreateRoom {
        name: String,
        topic: Option<String>,
        is_encrypted: bool,
    },
    JoinRoom {
        room_id: OwnedRoomId,
    },
    LeaveRoom {
        room_id: OwnedRoomId,
    },
    SetTyping {
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
    LoginFailure(String),
    SyncError(String),
    RoomListDiff(Vec<RoomListEntryDiff>),
    RoomDetailsUpdate {
        room_id: OwnedRoomId,
        details: RoomDetails,
    },
    TimelineDiff {
        room_id: OwnedRoomId,
        diff: Vec<TimelineDiff>,
    },
}

pub struct MatrixWorker {
    actor: Rc<RefCell<MatrixActor>>,
    bridge_id: Rc<RefCell<Option<HandlerId>>>,
}

impl Worker for MatrixWorker {
    type Input = ToActor;
    type Output = ToShell;
    type Message = ();

    fn create(scope: &WorkerScope<Self>) -> Self {
        let (tx, mut rx) = mpsc::unbounded();
        let actor = Rc::new(RefCell::new(MatrixActor::new(tx)));
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
            let responses = actor.borrow_mut().handle_message(msg).await;
            for response in responses {
                scope.respond(id, response);
            }
        });
    }
}

struct MatrixActor {
    client: Option<Client>,
    event_sender: mpsc::UnboundedSender<ToShell>,
    active_timelines: HashMap<OwnedRoomId, Arc<Timeline>>,
}

impl MatrixActor {
    fn new(event_sender: mpsc::UnboundedSender<ToShell>) -> Self {
        Self {
            client: None,
            event_sender,
            active_timelines: HashMap::new(),
        }
    }

    fn send_event(&self, event: ToShell) {
        let _ = self.event_sender.unbounded_send(event);
    }

    async fn handle_message(&mut self, msg: ToActor) -> Vec<ToShell> {
        match msg {
            ToActor::Login {
                homeserver_url,
                username,
                password,
            } => self.login(homeserver_url, username, password).await,
            ToActor::RestoreSession => self.restore_session().await,
            ToActor::StartSync => {
                self.start_sync().await;
                vec![]
            }
            ToActor::OpenRoom { room_id } => {
                self.open_room(room_id).await;
                vec![]
            }
            ToActor::CloseRoom { room_id } => {
                self.active_timelines.remove(&room_id);
                vec![]
            }
            ToActor::SendMessage {
                room_id,
                body,
                reply_to: _,
            } => {
                self.send_message(room_id, body).await;
                vec![]
            }
            ToActor::SendMedia {
                room_id,
                filename,
                mime_type,
                data,
            } => {
                self.send_media(room_id, filename, mime_type, data).await;
                vec![]
            }
            ToActor::SetTyping { room_id, typing } => {
                if let Some(client) = &self.client {
                    if let Some(room) = client.get_room(&room_id) {
                        let _ = room.typing_notice(typing).await;
                    }
                }
                vec![]
            }
            ToActor::JoinRoom { room_id } => {
                if let Some(client) = &self.client {
                    let _ = client.join_room_by_id(&room_id).await;
                }
                vec![]
            }
            ToActor::LeaveRoom { room_id } => {
                if let Some(client) = &self.client {
                    if let Some(room) = client.get_room(&room_id) {
                        let _ = room.leave().await;
                        self.active_timelines.remove(&room_id);
                    }
                }
                vec![]
            }
            ToActor::CreateRoom {
                name,
                topic,
                is_encrypted: _,
            } => {
                if let Some(client) = &self.client {
                    use matrix_sdk::ruma::api::client::room::create_room::v3::Request as CreateRoomRequest;

                    let mut request = CreateRoomRequest::new();
                    request.name = Some(name);
                    request.topic = topic;

                    // TODO: Add an m.room.encryption state event to `request.initial_state`
                    let _ = client.create_room(request).await;
                }
                vec![]
            }
            ToActor::LoadHistory { room_id } => {
                if let Some(timeline) = self.active_timelines.get(&room_id) {
                    let _ = timeline.paginate_backwards(20).await;
                }
                vec![]
            }
        }
    }

    async fn login(&mut self, url: String, user: String, pass: String) -> Vec<ToShell> {
        let client_builder = Client::builder()
            .homeserver_url(&url)
            .indexeddb_store("selvedge-store", None);

        match client_builder.build().await {
            Ok(client) => match client.matrix_auth().login_username(&user, &pass).await {
                Ok(_) => {
                    self.client = Some(client);
                    vec![ToShell::LoginSuccess]
                }
                Err(e) => vec![ToShell::LoginFailure(e.to_string())],
            },
            Err(e) => vec![ToShell::LoginFailure(e.to_string())],
        }
    }

    async fn restore_session(&mut self) -> Vec<ToShell> {
        let client_builder = Client::builder().indexeddb_store("selvedge-store", None);

        match client_builder.build().await {
            Ok(client) => {
                if client.session_meta().is_some() {
                    self.client = Some(client);
                    vec![ToShell::LoginSuccess]
                } else {
                    vec![ToShell::LoginFailure("No saved session found".to_string())]
                }
            }
            Err(e) => vec![ToShell::LoginFailure(e.to_string())],
        }
    }

    async fn start_sync(&self) {
        if let Some(client) = &self.client {
            let client = client.clone();
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
                                while let Some(_) = sync_stream.next().await {}
                            });
                        }

                        {
                            let svc = room_list_service.clone();
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
                        let _ = sender.unbounded_send(ToShell::SyncError(e.to_string()));
                    }
                }
            });
        }
    }

    async fn open_room(&mut self, room_id: OwnedRoomId) {
        if let Some(client) = &self.client {
            if let Some(room) = client.get_room(&room_id) {
                if !self.active_timelines.contains_key(&room_id) {
                    if let Ok(timeline) = room.timeline_builder().build().await {
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
                            .insert(room_id.clone(), Arc::new(timeline));

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
        }
    }

    async fn send_message(&self, room_id: OwnedRoomId, body: String) {
        if let Some(timeline) = self.active_timelines.get(&room_id) {
            let content = RoomMessageEventContent::text_plain(body);
            let _ = timeline.send(content.into()).await;
        } else if let Some(client) = &self.client {
            if let Some(room) = client.get_room(&room_id) {
                let content = RoomMessageEventContent::text_plain(body);
                let _ = room.send(content).await;
            }
        }
    }

    async fn send_media(
        &self,
        room_id: OwnedRoomId,
        filename: String,
        mime_type: String,
        data: Vec<u8>,
    ) {
        if let Some(client) = &self.client {
            if let Some(room) = client.get_room(&room_id) {
                let config = AttachmentConfig::new();
                if let Ok(mime) = mime::Mime::from_str(&mime_type) {
                    let _ = room.send_attachment(&filename, &mime, data, config).await;
                }
            }
        }
    }
}

fn map_timeline_item_safe(item: &matrix_sdk_ui::timeline::TimelineItem) -> TimelineItem {
    match item.kind() {
        matrix_sdk_ui::timeline::TimelineItemKind::Event(event) => {
            let content = if let Some(msg) = event.content().as_message() {
                let ruma_content = RoomMessageEventContent::new(msg.msgtype().clone());
                ruma_content.into()
            } else {
                // Ignore State Events / unsupported for now
                TimelineContent::Unsupported
            };

            let delivery_status = if let Some(local_echo) = event.send_state() {
                match local_echo {
                    EventSendState::NotSentYet { .. } => {
                        DeliveryStatus::Sending(OwnedTransactionId::from("dummy"))
                    }
                    EventSendState::Sent { .. } => DeliveryStatus::Sent,
                    EventSendState::SendingFailed { .. } => {
                        DeliveryStatus::Error("Failed to send".to_string())
                    }
                }
            } else {
                DeliveryStatus::Synced
            };

            let event_id = event
                .event_id()
                .map(|id| id.to_owned())
                .unwrap_or_else(|| OwnedEventId::from_str("$dummy").unwrap());

            let is_edited = event
                .content()
                .as_message()
                .map(|m| m.is_edited())
                .unwrap_or(false);

            TimelineItem::Event(EventItem {
                event_id,
                sender: event.sender().to_owned(),
                sender_profile: None,
                timestamp: MilliSecondsSinceUnixEpoch(event.timestamp().0.into()),
                content,
                reactions: Default::default(),
                read_receipts: Default::default(),
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
                    ts: MilliSecondsSinceUnixEpoch(ts.0.into()),
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

async fn room_list_item_to_view(item: RoomListItem) -> RoomListEntryView {
    let unread = item.unread_notification_counts();

    let last_activity = item
        .latest_event()
        .and_then(|e| e.event().timestamp())
        .unwrap_or_else(|| MilliSecondsSinceUnixEpoch(0u32.into()));

    let summary = RoomSummary {
        room_id: item.room_id().to_owned(),
        name: item.name(),
        avatar_url: item.avatar_url().map(|a| a.to_owned()),
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

async fn map_room_list_diff(diff: VectorDiff<RoomListItem>) -> RoomListEntryDiff {
    match diff {
        VectorDiff::Append { values } => RoomListEntryDiff::Append {
            entries: futures::future::join_all(
                values.into_iter().map(|v| room_list_item_to_view(v)),
            )
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
            entries: futures::future::join_all(
                values.into_iter().map(|v| room_list_item_to_view(v)),
            )
            .await,
        },
    }
}
