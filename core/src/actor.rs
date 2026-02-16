use matrix_sdk::sync::SyncResponse;
use ruma::api::client::receipt::create_receipt::v3::ReceiptType;
use ruma::events::room::encrypted::SyncRoomEncryptedEvent;
use ruma::events::room::message::SyncRoomMessageEvent;
use ruma::events::room::redaction::SyncRoomRedactionEvent;
use ruma::events::{AnySyncEphemeralRoomEvent, SyncStateEvent};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::Duration;

use gloo_worker::{HandlerId, Worker, WorkerScope};
use matrix_sdk::{
    Client,
    config::SyncSettings,
    ruma::{
        EventId, RoomId,
        events::{
            AnySyncMessageLikeEvent, AnySyncStateEvent, AnySyncTimelineEvent,
            receipt::ReceiptThread,
            room::message::{Relation, RoomMessageEventContent},
        },
    },
};
use selvedge_shared::{MessageView, RoomView, WorkerCommand, WorkerEvent};
use wasm_bindgen_futures::spawn_local;

use crate::model::{ChatStore, Message, MessageEvent, MessageTimeStamp, RoomInfo};
use crate::view::{to_message_view, to_room_view, to_sticker_pack_view};

pub enum InternalMsg {
    LoginSuccess(Client),
    LoginError(String),
    SyncResponse(SyncResponse),
    SyncError(String),
    CommandError(HandlerId, String),
    CommandSuccess(HandlerId, WorkerEvent),
}

pub struct CoreWorker {
    client: Option<Client>,
    store: Rc<RefCell<ChatStore>>,
    subscribers: HashSet<HandlerId>,
}

impl Worker for CoreWorker {
    type Message = InternalMsg;
    type Input = WorkerCommand;
    type Output = WorkerEvent;

    fn create(_scope: &WorkerScope<Self>) -> Self {
        Self {
            client: None,
            store: Rc::new(RefCell::new(ChatStore::default())),
            subscribers: HashSet::new(),
        }
    }

    fn connected(&mut self, _scope: &WorkerScope<Self>, id: HandlerId) {
        self.subscribers.insert(id);
    }

    fn disconnected(&mut self, _scope: &WorkerScope<Self>, id: HandlerId) {
        self.subscribers.remove(&id);
    }

    fn update(&mut self, scope: &WorkerScope<Self>, msg: Self::Message) {
        match msg {
            InternalMsg::LoginSuccess(client) => {
                self.client = Some(client.clone());

                let store = self.store.clone();
                let scope_clone = scope.clone();

                spawn_local(async move {
                    sync_loop(client, store, scope_clone).await;
                });
            }
            InternalMsg::LoginError(_err) => {
                self.client = None;
            }
            InternalMsg::SyncResponse(response) => {
                let updates = {
                    let mut store = self.store.borrow_mut();
                    process_sync_response(&mut store, &response, self.client.as_ref())
                };

                if !updates.room_updates.is_empty() {
                    let event = WorkerEvent::RoomListUpdate(updates.room_updates);
                    self.broadcast(scope, event);
                }

                for (room_id, messages) in updates.timeline_updates {
                    let event = WorkerEvent::TimelineUpdate {
                        room_id,
                        events: messages,
                        clear_cache: false,
                    };
                    self.broadcast(scope, event);
                }

                self.broadcast(
                    scope,
                    WorkerEvent::SyncStatus {
                        stage: "Synced".into(),
                        progress: None,
                    },
                );
            }
            InternalMsg::SyncError(err) => {
                gloo_console::error!("Sync error:", &err);
                self.broadcast(scope, WorkerEvent::Error(format!("Sync error: {}", err)));
            }
            InternalMsg::CommandError(id, err) => {
                scope.respond(id, WorkerEvent::Error(err));
            }
            InternalMsg::CommandSuccess(id, event) => {
                scope.respond(id, event);
            }
        }
    }

    fn received(&mut self, scope: &WorkerScope<Self>, msg: Self::Input, id: HandlerId) {
        match msg {
            WorkerCommand::Login { user, pass } => {
                let scope = scope.clone();
                spawn_local(async move {
                    let client_builder =
                        Client::builder().homeserver_url("https://matrix-client.matrix.org");

                    match client_builder.build().await {
                        Ok(client) => {
                            match client
                                .matrix_auth()
                                .login_username(&user, &pass)
                                .send()
                                .await
                            {
                                Ok(_) => {
                                    scope.respond(id, WorkerEvent::LoginSuccess);
                                    scope.send_message(InternalMsg::LoginSuccess(client));
                                }
                                Err(e) => {
                                    let err = e.to_string();
                                    scope.respond(id, WorkerEvent::Error(err.clone()));
                                    scope.send_message(InternalMsg::LoginError(err));
                                }
                            }
                        }
                        Err(e) => scope.respond(id, WorkerEvent::Error(e.to_string())),
                    }
                });
            }

            WorkerCommand::Logout => {
                self.client = None;
                self.store.borrow_mut().rooms.clear();
                self.broadcast(scope, WorkerEvent::LoggedOut);
            }

            WorkerCommand::InitSync => {
                if self.client.is_some() {
                    scope.respond(
                        id,
                        WorkerEvent::SyncStatus {
                            stage: "Running".into(),
                            progress: None,
                        },
                    );
                }
            }

            WorkerCommand::LoadRoom { room_id } => {
                if let Ok(rid) = RoomId::parse(&room_id) {
                    if let Some(info) = self.store.borrow().rooms.get(&rid) {
                        if let Some(client) = &self.client {
                            if let Some(user_id) = client.user_id() {
                                let view = to_room_view(&rid, info, user_id);
                                scope.respond(id, WorkerEvent::RoomListUpdate(vec![view]));

                                let messages: Vec<MessageView> = info
                                    .messages
                                    .map
                                    .values()
                                    .rev()
                                    .take(50)
                                    .map(|m| to_message_view(m, info, user_id))
                                    .collect();

                                scope.respond(
                                    id,
                                    WorkerEvent::TimelineUpdate {
                                        room_id,
                                        events: messages.into_iter().rev().collect(),
                                        clear_cache: true,
                                    },
                                );
                            }
                        }
                    }
                }
            }

            WorkerCommand::LoadHistory {
                room_id: _,
                limit: _,
            } => {}

            WorkerCommand::SendMessage {
                room_id,
                body,
                html,
                reply_to_id,
            } => {
                if let Some(client) = &self.client {
                    let client = client.clone();
                    let scope = scope.clone();

                    spawn_local(async move {
                        if let Ok(rid) = RoomId::parse(&room_id) {
                            if let Some(room) = client.get_room(&rid) {
                                let mut content = if let Some(html_body) = html {
                                    RoomMessageEventContent::text_html(body, html_body)
                                } else {
                                    RoomMessageEventContent::text_plain(body)
                                };

                                if let Some(reply_id) = reply_to_id {
                                    if let Ok(event_id) = EventId::parse(reply_id) {
                                        content.relates_to = Some(Relation::Reply {
                                            in_reply_to:
                                                matrix_sdk::ruma::events::relation::InReplyTo::new(
                                                    event_id,
                                                ),
                                        });
                                    }
                                }

                                if let Err(e) = room.send(content).await {
                                    scope.send_message(InternalMsg::CommandError(
                                        id,
                                        format!("Send failed: {}", e),
                                    ));
                                }
                            }
                        }
                    });
                }
            }

            WorkerCommand::SendReaction {
                room_id,
                event_id,
                key,
            } => {
                if let Some(client) = &self.client {
                    let client = client.clone();
                    let scope = scope.clone();
                    spawn_local(async move {
                        if let Ok(rid) = RoomId::parse(&room_id) {
                            if let Some(room) = client.get_room(&rid) {
                                if let Ok(eid) = EventId::parse(event_id) {
                                    let content =
                                        matrix_sdk::ruma::events::reaction::ReactionEventContent::new(
                                            matrix_sdk::ruma::events::relation::Annotation::new(
                                                eid, key,
                                            ),
                                        );
                                    if let Err(e) = room.send(content).await {
                                        scope.send_message(InternalMsg::CommandError(
                                            id,
                                            format!("React failed: {}", e),
                                        ));
                                    }
                                }
                            }
                        }
                    });
                }
            }

            WorkerCommand::EditMessage {
                room_id,
                event_id,
                new_body,
            } => {
                if let Some(client) = &self.client {
                    let client = client.clone();
                    let scope = scope.clone();
                    spawn_local(async move {
                        if let Ok(rid) = RoomId::parse(&room_id) {
                            if let Some(room) = client.get_room(&rid) {
                                if let Ok(eid) = EventId::parse(event_id) {
                                    let mut content = RoomMessageEventContent::text_plain(new_body);
                                    content.relates_to = Some(Relation::Replacement(
                                        matrix_sdk::ruma::events::relation::Replacement::new(
                                            eid,
                                            content.clone().into(),
                                        ),
                                    ));
                                    if let Err(e) = room.send(content).await {
                                        scope.send_message(InternalMsg::CommandError(
                                            id,
                                            format!("Edit failed: {}", e),
                                        ));
                                    }
                                }
                            }
                        }
                    });
                }
            }

            WorkerCommand::RedactMessage {
                room_id,
                event_id,
                reason,
            } => {
                if let Some(client) = &self.client {
                    let client = client.clone();
                    let scope = scope.clone();
                    spawn_local(async move {
                        if let Ok(rid) = RoomId::parse(&room_id) {
                            if let Some(room) = client.get_room(&rid) {
                                if let Ok(eid) = EventId::parse(event_id) {
                                    if let Err(e) = room.redact(&eid, reason.as_deref(), None).await
                                    {
                                        scope.send_message(InternalMsg::CommandError(
                                            id,
                                            format!("Redact failed: {}", e),
                                        ));
                                    }
                                }
                            }
                        }
                    });
                }
            }

            WorkerCommand::SetTyping { room_id, typing } => {
                if let Some(client) = &self.client {
                    let client = client.clone();
                    spawn_local(async move {
                        if let Ok(rid) = RoomId::parse(&room_id) {
                            if let Some(room) = client.get_room(&rid) {
                                let _ = room.typing_notice(typing).await;
                            }
                        }
                    });
                }
            }

            WorkerCommand::MarkRead { room_id, event_id } => {
                if let Some(client) = &self.client {
                    let client = client.clone();

                    spawn_local(async move {
                        if let Ok(rid) = RoomId::parse(&room_id) {
                            if let Some(room) = client.get_room(&rid) {
                                if let Ok(eid) = EventId::parse(event_id) {
                                    let _ = room
                                        .send_single_receipt(
                                            ReceiptType::Read,
                                            ReceiptThread::Unthreaded,
                                            eid,
                                        )
                                        .await;
                                }
                            }
                        }
                    });
                }
            }

            WorkerCommand::FetchStickerPacks => {
                let store = self.store.borrow();
                let views = store
                    .sticker_packs
                    .iter()
                    .map(to_sticker_pack_view)
                    .collect();
                scope.respond(id, WorkerEvent::StickerPacksReady(views));
            }
        }
    }
}

impl CoreWorker {
    fn broadcast(&self, scope: &WorkerScope<Self>, event: WorkerEvent) {
        for sub in &self.subscribers {
            scope.respond(*sub, event.clone());
        }
    }
}

async fn sync_loop(client: Client, _store: Rc<RefCell<ChatStore>>, scope: WorkerScope<CoreWorker>) {
    let settings = SyncSettings::new();

    loop {
        match client.sync_once(settings.clone()).await {
            Ok(response) => {
                scope.send_message(InternalMsg::SyncResponse(response));
            }
            Err(e) => {
                scope.send_message(InternalMsg::SyncError(e.to_string()));
                gloo_timers::future::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

struct SyncUpdates {
    room_updates: Vec<RoomView>,
    timeline_updates: HashMap<String, Vec<MessageView>>,
}

fn process_sync_response(
    store: &mut ChatStore,
    response: &SyncResponse,
    client: Option<&Client>,
) -> SyncUpdates {
    let user_id = if let Some(c) = client {
        c.user_id().unwrap()
    } else {
        return SyncUpdates {
            room_updates: vec![],
            timeline_updates: HashMap::new(),
        };
    };

    let mut room_updates = Vec::new();
    let mut timeline_updates = HashMap::new();

    for (room_id, room_info) in &response.rooms.joined {
        let mut new_room = false;
        let info = store.rooms.entry(room_id.clone()).or_insert_with(|| {
            new_room = true;
            RoomInfo::default()
        });

        let mut new_messages = Vec::new();

        for event in &room_info.timeline.events {
            if let Ok(any_event) = event.raw().deserialize() {
                match any_event {
                    AnySyncTimelineEvent::MessageLike(msg_event) => {
                        let event_id = msg_event.event_id().to_owned();
                        let timestamp =
                            MessageTimeStamp::OriginServer(msg_event.origin_server_ts());
                        let sender = msg_event.sender().to_owned();

                        if let AnySyncMessageLikeEvent::RoomRedaction(
                            SyncRoomRedactionEvent::Original(_redaction),
                        ) = &msg_event
                        {
                            // TODO: info.redact_event(redaction.redacts)
                        }

                        let internal_event = match msg_event {
                            AnySyncMessageLikeEvent::RoomMessage(
                                SyncRoomMessageEvent::Original(ev),
                            ) => Some(MessageEvent::Original(Box::new(
                                ruma::events::SyncMessageLikeEvent::Original(ev),
                            ))),
                            AnySyncMessageLikeEvent::RoomEncrypted(
                                SyncRoomEncryptedEvent::Original(ev),
                            ) => Some(MessageEvent::EncryptedOriginal(Box::new(
                                ruma::events::SyncMessageLikeEvent::Original(ev),
                            ))),
                            _ => None,
                        };

                        if let Some(evt) = internal_event {
                            let msg = Message {
                                event: evt,
                                sender,
                                timestamp: timestamp.clone(),
                                formatted_body: None,
                                downloaded: true,
                            };

                            info.messages
                                .map
                                .insert((timestamp, event_id.clone()), msg.clone());

                            new_messages.push(to_message_view(&msg, info, user_id));
                        }
                    }
                    AnySyncTimelineEvent::State(state_event) => match state_event {
                        AnySyncStateEvent::RoomName(SyncStateEvent::Original(ev)) => {
                            info.name = Some(ev.content.name);
                        }
                        AnySyncStateEvent::RoomTopic(SyncStateEvent::Original(ev)) => {
                            info.topic = Some(ev.content.topic);
                        }
                        _ => {}
                    },
                }
            }
        }

        for event in &room_info.ephemeral {
            if let Ok(ephemeral) = event.deserialize() {
                match ephemeral {
                    AnySyncEphemeralRoomEvent::Typing(ev) => {
                        let now = js_sys::Date::now() as u64;
                        info.users_typing = Some((now + 5000, ev.content.user_ids));
                    }
                    AnySyncEphemeralRoomEvent::Receipt(ev) => {
                        for (event_id, receipts) in ev.content.0 {
                            if let Some(read) =
                                receipts.get(&ruma::events::receipt::ReceiptType::Read)
                            {
                                for (user_id, _) in read {
                                    info.event_receipts
                                        .entry(ReceiptThread::Main)
                                        .or_default()
                                        .entry(event_id.clone())
                                        .or_default()
                                        .insert(user_id.clone());
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if !new_messages.is_empty() {
            timeline_updates.insert(room_id.to_string(), new_messages);
        }

        let state_is_empty = match &room_info.state {
            matrix_sdk::sync::State::Before(v) => v.is_empty(),
            matrix_sdk::sync::State::After(v) => v.is_empty(),
        };

        if new_room || !state_is_empty {
            room_updates.push(to_room_view(room_id, info, user_id));
        }
    }

    SyncUpdates {
        room_updates,
        timeline_updates,
    }
}
