use crate::model::{
    DeliveryStatus, EventItem, MemberProfile, RoomDetails, RoomSummary, TimelineContent,
    TimelineItem,
};
use futures::{StreamExt, channel::mpsc};
use gloo_worker::HandlerId;
use gloo_worker::{Worker, WorkerScope};
use matrix_sdk::{
    Client,
    attachment::AttachmentConfig,
    config::SyncSettings,
    ruma::{
        OwnedEventId, OwnedRoomId,
        events::{
            AnyMessageLikeEventContent, AnySyncTimelineEvent,
            room::message::RoomMessageEventContent,
        },
    },
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::str::FromStr;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Serialize, Deserialize)]
pub enum ToActor {
    Login {
        homeserver_url: String,
        username: String,
        password: String,
    },
    StartSync,
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
    RoomListUpdate(Vec<RoomSummary>),
    RoomDetailsUpdate {
        room_id: OwnedRoomId,
        details: RoomDetails,
    },
    TimelineEvent {
        room_id: OwnedRoomId,
        item: TimelineItem,
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
}

impl MatrixActor {
    fn new(event_sender: mpsc::UnboundedSender<ToShell>) -> Self {
        Self {
            client: None,
            event_sender,
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
            ToActor::StartSync => {
                self.start_sync().await;
                vec![]
            }
            ToActor::SendMessage {
                room_id,
                body,
                reply_to,
            } => {
                self.send_message(room_id, body, reply_to).await;
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
                    // Trigger a refresh of room list
                    // self.refresh_room_list().await;
                }
                vec![]
            }
            _ => vec![],
        }
    }

    async fn login(&mut self, url: String, user: String, pass: String) -> Vec<ToShell> {
        let client_builder = Client::builder().homeserver_url(&url);

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

    async fn start_sync(&self) {
        if let Some(client) = &self.client {
            let client = client.clone();
            let sender = self.event_sender.clone();

            spawn_local(async move {
                let settings = SyncSettings::default();

                client.add_event_handler({
                    let sender = sender.clone();
                    move |event: AnySyncTimelineEvent, room: matrix_sdk::Room| {
                        let sender = sender.clone();
                        async move {
                            let timeline_item = Self::process_incoming_event(event, &room).await;

                            if let Some(item) = timeline_item {
                                let _ = sender.unbounded_send(ToShell::TimelineEvent {
                                    room_id: room.room_id().to_owned(),
                                    item,
                                });
                            }
                        }
                    }
                });

                if let Err(e) = client.sync(settings).await {
                    let _ = sender.unbounded_send(ToShell::SyncError(e.to_string()));
                }
            });
        }
    }

    async fn send_message(
        &self,
        room_id: OwnedRoomId,
        body: String,
        _reply_to: Option<OwnedEventId>,
    ) {
        if let Some(client) = &self.client {
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

    async fn process_incoming_event(
        event: AnySyncTimelineEvent,
        room: &matrix_sdk::Room,
    ) -> Option<TimelineItem> {
        match event {
            AnySyncTimelineEvent::MessageLike(msg) => {
                let event_id = msg.event_id().to_owned();
                let sender = msg.sender().to_owned();
                let timestamp = msg.origin_server_ts();

                let member_profile = match room.get_member(&sender).await {
                    Ok(Some(member)) => Some(MemberProfile {
                        user_id: sender.clone(),
                        display_name: member.display_name().map(ToOwned::to_owned),
                        avatar_url: member.avatar_url().map(ToOwned::to_owned),
                        membership: member.membership().clone(),
                    }),
                    _ => None,
                };

                if let Some(content_enum) = msg.original_content() {
                    let content: TimelineContent = match content_enum {
                        AnyMessageLikeEventContent::RoomMessage(c) => c.into(),
                        // TODO:  handle stickers later
                        _ => TimelineContent::Unsupported,
                    };

                    return Some(TimelineItem::Event(EventItem {
                        event_id,
                        sender,
                        sender_profile: member_profile,
                        timestamp,
                        content,
                        reactions: Default::default(),
                        delivery_status: DeliveryStatus::Synced,
                        in_reply_to: None,
                        is_edited: false,
                    }));
                }
            }
            // TODO: handle state events (RoomName, Topic, etc)
            _ => {}
        }
        None
    }
}
