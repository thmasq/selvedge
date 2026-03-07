use crate::model::{
    ActorError, DeliveryStatus, EventItem, ModelError, RoomDetails, RoomListEntryDiff,
    RoomListEntryView, RoomSummary, TimelineContent, TimelineDiff, TimelineItem, VerificationState,
};
use eyeball_im::VectorDiff;
use futures::{StreamExt, channel::mpsc};
use gloo_worker::HandlerId;
use gloo_worker::{Worker, WorkerScope};
use indexmap::IndexMap;
use matrix_sdk::{
    Client,
    attachment::AttachmentConfig,
    encryption::verification::{QrVerification, SasVerification},
    ruma::{
        EventEncryptionAlgorithm, OwnedEventId, OwnedRoomId, OwnedTransactionId, OwnedUserId,
        events::AnyInitialStateEvent, events::ToDeviceEvent,
        events::key::verification::request::ToDeviceKeyVerificationRequestEventContent,
        events::room::encryption::RoomEncryptionEventContent,
        events::room::message::RoomMessageEventContent, serde::Raw,
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
    RequestVerification {
        request_id: String,
        user_id: OwnedUserId,
    },
    AcceptVerification {
        request_id: String,
        user_id: OwnedUserId,
        flow_id: String,
    },
    ConfirmVerification {
        request_id: String,
        user_id: OwnedUserId,
        flow_id: String,
        emojis_match: bool,
    },
    CancelVerification {
        request_id: String,
        user_id: OwnedUserId,
        flow_id: String,
    },
    SetupRecovery {
        request_id: String,
        passphrase: String,
    },
    SubmitUiaResponse {
        request_id: String,
        session: String,
        password: String,
        passphrase: String,
    },
    RecoverIdentity {
        request_id: String,
        passphrase: String,
    },
    EnableKeyBackup {
        request_id: String,
        passphrase: String,
    },
    RestoreKeyBackup {
        request_id: String,
        passphrase: String,
    },
    RetryDecryption {
        request_id: String,
        room_id: OwnedRoomId,
        session_id: String,
    },
    ExportKeys {
        request_id: String,
        passphrase: String,
    },
    ImportKeys {
        request_id: String,
        passphrase: String,
        payload: Vec<u8>,
    },
    GenerateQrCode {
        request_id: String,
        user_id: OwnedUserId,
        flow_id: String,
    },
    ConfirmQrScan {
        request_id: String,
        user_id: OwnedUserId,
        flow_id: String,
        scanned_data: Vec<u8>,
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
    UiaaPrompt {
        request_id: String,
        session: String,
    },
    VerificationUpdate {
        user_id: OwnedUserId,
        flow_id: String,
        state: VerificationState,
    },
    KeysExported {
        request_id: String,
        payload: String,
    },
    QrCodeGenerated {
        request_id: String,
        payload: Vec<u8>,
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
    active_sas_verifications: RefCell<HashMap<String, SasVerification>>,
    active_qr_verifications: RefCell<HashMap<String, QrVerification>>,
}

impl MatrixActor {
    fn new(event_sender: mpsc::UnboundedSender<ToShell>) -> Self {
        Self {
            client: RefCell::new(None),
            event_sender,
            active_timelines: RefCell::new(HashMap::new()),
            active_sas_verifications: RefCell::new(HashMap::new()),
            active_qr_verifications: RefCell::new(HashMap::new()),
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
                is_encrypted,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    let mut request =
                        matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
                    request.name = Some(name);
                    request.topic = topic;

                    if is_encrypted {
                        let content = RoomEncryptionEventContent::new(
                            EventEncryptionAlgorithm::MegolmV1AesSha2,
                        );

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
            ToActor::RequestVerification {
                request_id,
                user_id,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    match client.encryption().request_user_identity(&user_id).await {
                        Ok(Some(user_identity)) => {
                            match user_identity.request_verification().await {
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
                        }
                        Ok(None) => ToShell::CommandResult {
                            request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(
                                "User identity not found on the server".into(),
                            )),
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
            ToActor::AcceptVerification {
                request_id,
                user_id,
                flow_id,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    if let Some(request) = client
                        .encryption()
                        .get_verification_request(&user_id, &flow_id)
                        .await
                    {
                        match request.accept().await {
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
                    } else if let Some(sas) = self
                        .active_sas_verifications
                        .borrow()
                        .get(&flow_id)
                        .cloned()
                    {
                        match sas.accept().await {
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
                            error: Some(ActorError::RoomOperationFailed(
                                "Verification flow not found".into(),
                            )),
                        }
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
            ToActor::ConfirmVerification {
                request_id,
                user_id: _,
                flow_id,
                emojis_match,
            } => {
                let verification = self
                    .active_sas_verifications
                    .borrow()
                    .get(&flow_id)
                    .cloned();
                let response = if let Some(sas) = verification {
                    let res = if emojis_match {
                        sas.confirm().await
                    } else {
                        sas.cancel().await
                    };
                    match res {
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
                        error: Some(ActorError::RoomOperationFailed(
                            "Verification flow not found".into(),
                        )),
                    }
                };
                vec![response]
            }
            ToActor::CancelVerification {
                request_id,
                user_id: _,
                flow_id,
            } => {
                let verification = self
                    .active_sas_verifications
                    .borrow()
                    .get(&flow_id)
                    .cloned();
                let response = if let Some(sas) = verification {
                    match sas.cancel().await {
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
                        error: Some(ActorError::RoomOperationFailed(
                            "Verification flow not found".into(),
                        )),
                    }
                };
                vec![response]
            }
            ToActor::SetupRecovery {
                request_id,
                passphrase,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    match client
                        .encryption()
                        .recovery()
                        .enable()
                        .with_passphrase(&passphrase)
                        .await
                    {
                        Ok(_) => ToShell::CommandResult {
                            request_id,
                            success: true,
                            error: None,
                        },
                        Err(e) => {
                            if let matrix_sdk::encryption::recovery::RecoveryError::Sdk(sdk_err) =
                                &e
                            {
                                if let Some(uiaa_info) = sdk_err.as_uiaa_response() {
                                    if let Some(session) = &uiaa_info.session {
                                        return vec![ToShell::UiaaPrompt {
                                            request_id,
                                            session: session.clone(),
                                        }];
                                    }
                                }
                            }

                            ToShell::CommandResult {
                                request_id,
                                success: false,
                                error: Some(ActorError::RoomOperationFailed(e.to_string())),
                            }
                        }
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
            ToActor::SubmitUiaResponse {
                request_id,
                session,
                password,
                passphrase,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    let identifier =
                        matrix_sdk::ruma::api::client::uiaa::UserIdentifier::UserIdOrLocalpart(
                            client
                                .user_id()
                                .map(|id| id.to_string())
                                .unwrap_or_default(),
                        );

                    let mut uiaa_password =
                        matrix_sdk::ruma::api::client::uiaa::Password::new(identifier, password);
                    uiaa_password.session = Some(session);

                    let auth_data =
                        matrix_sdk::ruma::api::client::uiaa::AuthData::Password(uiaa_password);

                    match client
                        .encryption()
                        .bootstrap_cross_signing(Some(auth_data))
                        .await
                    {
                        Ok(_) => {
                            match client
                                .encryption()
                                .recovery()
                                .enable()
                                .with_passphrase(&passphrase)
                                .await
                            {
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
            ToActor::RecoverIdentity {
                request_id,
                passphrase,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    match client.encryption().recovery().recover(&passphrase).await {
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
            ToActor::EnableKeyBackup {
                request_id,
                passphrase: _,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    match client.encryption().backups().create().await {
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
            ToActor::RestoreKeyBackup {
                request_id,
                passphrase,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    match client.encryption().recovery().recover(&passphrase).await {
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
            ToActor::RetryDecryption {
                request_id,
                room_id,
                session_id,
            } => {
                let timeline = self.active_timelines.borrow().get(&room_id).cloned();
                let response = if let Some(timeline) = timeline {
                    timeline
                        .retry_decryption(std::iter::once(session_id.as_str()))
                        .await;

                    ToShell::CommandResult {
                        request_id,
                        success: true,
                        error: None,
                    }
                } else {
                    ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(
                            "Timeline not found for the given room".into(),
                        )),
                    }
                };
                vec![response]
            }
            ToActor::ExportKeys {
                request_id,
                passphrase: _,
            } => {
                let response = ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(
                        "Manual file export is not supported in the web environment. Please use Account Recovery / Key Backup instead.".into(),
                    )),
                };
                vec![response]
            }
            ToActor::ImportKeys {
                request_id,
                passphrase: _,
                payload: _,
            } => {
                let response = ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(
                        "Manual file import is not supported in the web environment. Please use Account Recovery / Key Backup instead.".into(),
                    )),
                };
                vec![response]
            }
            ToActor::GenerateQrCode {
                request_id,
                user_id,
                flow_id,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    if let Some(request) = client
                        .encryption()
                        .get_verification_request(&user_id, &flow_id)
                        .await
                    {
                        match request.generate_qr_code().await {
                            Ok(Some(qr)) => {
                                let bytes = qr.to_bytes().unwrap_or_default();
                                self.active_qr_verifications
                                    .borrow_mut()
                                    .insert(flow_id.clone(), qr);
                                ToShell::QrCodeGenerated {
                                    request_id,
                                    payload: bytes,
                                }
                            }
                            Ok(None) => ToShell::CommandResult {
                                request_id,
                                success: false,
                                error: Some(ActorError::RoomOperationFailed(
                                    "Could not generate QR code (not supported or invalid state)"
                                        .into(),
                                )),
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
                            error: Some(ActorError::RoomOperationFailed(
                                "Verification flow not found".into(),
                            )),
                        }
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

            ToActor::ConfirmQrScan {
                request_id,
                user_id,
                flow_id,
                scanned_data,
            } => {
                let client = self.client.borrow().clone();
                let response = if let Some(client) = client {
                    if let Some(request) = client
                        .encryption()
                        .get_verification_request(&user_id, &flow_id)
                        .await
                    {
                        match matrix_sdk::encryption::verification::QrVerificationData::from_bytes(
                            &scanned_data,
                        ) {
                            Ok(qr_data) => match request.scan_qr_code(qr_data).await {
                                Ok(Some(qr)) => match qr.confirm().await {
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
                                },
                                Ok(None) => ToShell::CommandResult {
                                    request_id,
                                    success: false,
                                    error: Some(ActorError::RoomOperationFailed(
                                        "Scanned QR code does not match this verification flow"
                                            .into(),
                                    )),
                                },
                                Err(e) => ToShell::CommandResult {
                                    request_id,
                                    success: false,
                                    error: Some(ActorError::RoomOperationFailed(e.to_string())),
                                },
                            },
                            Err(e) => ToShell::CommandResult {
                                request_id,
                                success: false,
                                error: Some(ActorError::RoomOperationFailed(format!(
                                    "Invalid QR data: {}",
                                    e
                                ))),
                            },
                        }
                    } else {
                        ToShell::CommandResult {
                            request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(
                                "Verification flow not found".into(),
                            )),
                        }
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

            let verification_sender = sender.clone();

            client.add_event_handler(
                move |ev: ToDeviceEvent<ToDeviceKeyVerificationRequestEventContent>,
                      _client: Client| {
                    let sender_for_async = verification_sender.clone();
                    async move {
                        let user_id = ev.sender.clone();
                        let flow_id = ev.content.transaction_id.to_string();

                        if let Some(request) = _client
                            .encryption()
                            .get_verification_request(&user_id, &flow_id)
                            .await
                        {
                            if request.is_cancelled() || request.is_done() {
                                return;
                            }

                            let _ = sender_for_async.unbounded_send(ToShell::VerificationUpdate {
                                user_id,
                                flow_id,
                                state: crate::model::VerificationState::Requested {
                                    methods: ev.content.methods.clone(),
                                },
                            });
                        }
                    }
                },
            );

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
                            let mapper_client = client.clone();

                            spawn_local(async move {
                                if let Ok(all_rooms) = svc.all_rooms().await {
                                    let (entries_stream, controller) =
                                        all_rooms.entries_with_dynamic_adapters(50);

                                    controller.set_filter(Box::new(new_filter_all(vec![])));

                                    futures::pin_mut!(entries_stream);

                                    while let Some(diffs) = entries_stream.next().await {
                                        let mapped = futures::future::join_all(
                                            diffs.into_iter().map(|diff| {
                                                map_room_list_diff(mapper_client.clone(), diff)
                                            }),
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

                            if !is_user_verified {
                                // TODO: Add local storage check for "Identity Changed" alert here.

                                if trust_level == crate::model::RoomTrustLevel::Trusted {
                                    trust_level = crate::model::RoomTrustLevel::Normal;
                                }
                            } else {
                                // TODO: Mark user as verified in local storage here to support the check above in the future.

                                if let Ok(devices) =
                                    client.encryption().get_user_devices(&user_id).await
                                {
                                    for device in devices.devices() {
                                        if !device.is_cross_signed_by_owner() {
                                            trust_level = crate::model::RoomTrustLevel::Warning;
                                            break;
                                        }
                                    }
                                }
                            }

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
            let mut content = match event.content() {
                matrix_sdk_ui::timeline::TimelineItemContent::MsgLike(msg) => {
                    if let Some(msg_content) = msg.as_message() {
                        let ruma_content =
                            RoomMessageEventContent::new(msg_content.msgtype().clone());
                        ruma_content.into()
                    } else {
                        TimelineContent::Unsupported
                    }
                }
                _ => TimelineContent::Unsupported,
            };

            if matches!(content, TimelineContent::Unsupported) {
                if let Some(raw_json) = event.latest_json() {
                    if let Ok(Some(event_type)) = raw_json.get_field::<String>("type") {
                        if event_type == "m.room.encrypted" {
                            let session_id = raw_json
                                .get_field::<serde_json::Value>("content")
                                .ok()
                                .flatten()
                                .and_then(|c| {
                                    c.get("session_id")
                                        .and_then(|s| s.as_str())
                                        .map(ToString::to_string)
                                })
                                .unwrap_or_default();

                            content = TimelineContent::Undecryptable { session_id };
                        }
                    }
                }
            }

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
async fn room_list_item_to_view(client: Client, item: RoomListItem) -> RoomListEntryView {
    let unread = item.unread_notification_counts();

    let last_activity = item
        .latest_event()
        .and_then(|e| e.event().timestamp())
        .unwrap_or_else(|| MilliSecondsSinceUnixEpoch(0u32.into()));

    let is_encrypted = if let Some(room) = client.get_room(item.room_id()) {
        room.encryption_state().is_encrypted()
    } else {
        false
    };

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
        is_encrypted,
        tags: HashSet::new(),
    };

    RoomListEntryView::Filled(summary)
}

#[allow(clippy::future_not_send)]
async fn map_room_list_diff(client: Client, diff: VectorDiff<RoomListItem>) -> RoomListEntryDiff {
    match diff {
        VectorDiff::Append { values } => RoomListEntryDiff::Append {
            entries: futures::future::join_all(
                values
                    .into_iter()
                    .map(|v| room_list_item_to_view(client.clone(), v)),
            )
            .await,
        },
        VectorDiff::Clear => RoomListEntryDiff::Clear,
        VectorDiff::PushFront { value } => RoomListEntryDiff::PushFront {
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::PushBack { value } => RoomListEntryDiff::PushBack {
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::PopFront => RoomListEntryDiff::PopFront,
        VectorDiff::PopBack => RoomListEntryDiff::PopBack,
        VectorDiff::Insert { index, value } => RoomListEntryDiff::Insert {
            index,
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::Set { index, value } => RoomListEntryDiff::Set {
            index,
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::Remove { index } => RoomListEntryDiff::Remove { index },
        VectorDiff::Truncate { length } => RoomListEntryDiff::Truncate { length },
        VectorDiff::Reset { values } => RoomListEntryDiff::Reset {
            entries: futures::future::join_all(
                values
                    .into_iter()
                    .map(|v| room_list_item_to_view(client.clone(), v)),
            )
            .await,
        },
    }
}
