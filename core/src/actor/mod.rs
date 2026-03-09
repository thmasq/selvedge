pub mod handlers;
pub mod mapping;
pub mod message;
pub mod search;
pub mod worker;

pub use worker::MatrixWorker;

use futures::channel::mpsc;
use matrix_sdk::Client;
use matrix_sdk::encryption::verification::{QrVerification, SasVerification};
use matrix_sdk::ruma::OwnedRoomId;
use matrix_sdk_ui::timeline::Timeline;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use message::{ToActor, ToShell};
use search::SearchIndex;

pub(crate) struct MatrixActor {
    pub(crate) client: RefCell<Option<Client>>,
    pub(crate) event_sender: mpsc::UnboundedSender<ToShell>,
    pub(crate) active_timelines: RefCell<HashMap<OwnedRoomId, Rc<Timeline>>>,
    pub(crate) active_sas_verifications: RefCell<HashMap<String, SasVerification>>,
    pub(crate) active_qr_verifications: RefCell<HashMap<String, QrVerification>>,
    pub(crate) search_index: Rc<RefCell<SearchIndex>>,
}

impl MatrixActor {
    pub(crate) fn new(event_sender: mpsc::UnboundedSender<ToShell>) -> Self {
        Self {
            client: RefCell::new(None),
            event_sender,
            active_timelines: RefCell::new(HashMap::new()),
            active_sas_verifications: RefCell::new(HashMap::new()),
            active_qr_verifications: RefCell::new(HashMap::new()),
            search_index: Rc::new(RefCell::new(SearchIndex::default())),
        }
    }

    pub(crate) fn send_event(&self, event: ToShell) {
        let _ = self.event_sender.unbounded_send(event);
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn handle_message(&self, msg: ToActor) -> Vec<ToShell> {
        match msg {
            // --- Auth Handlers ---
            ToActor::Login {
                homeserver_url,
                username,
                password,
            } => self.login(homeserver_url, username, password).await,
            ToActor::RestoreSession => self.restore_session().await,

            // --- Sync Handlers ---
            ToActor::StartSync => {
                self.start_sync();
                vec![] // Sync runs in the background
            }

            // --- Room Lifecycle Handlers ---
            ToActor::OpenRoom { room_id } => {
                self.open_room(room_id).await;
                vec![] // Updates flow via background stream
            }
            ToActor::CloseRoom { room_id } => {
                self.close_room(room_id);
                vec![]
            }
            ToActor::JoinRoom {
                request_id,
                room_id,
            } => self.join_room(request_id, room_id).await,
            ToActor::LeaveRoom {
                request_id,
                room_id,
            } => self.leave_room(request_id, room_id).await,
            ToActor::CreateRoom {
                request_id,
                name,
                topic,
                is_encrypted,
            } => {
                self.create_room(request_id, name, topic, is_encrypted)
                    .await
            }
            ToActor::SetTyping {
                request_id,
                room_id,
                typing,
            } => self.set_typing(request_id, room_id, typing).await,
            ToActor::LoadHistory { room_id } => self.load_history(room_id).await,

            // --- Messaging Handlers ---
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
            ToActor::SearchMessages {
                request_id,
                room_id,
                query,
                limit,
            } => {
                self.search_messages(request_id, room_id, query, limit)
                    .await
            }

            // --- Crypto & Verification Handlers ---
            ToActor::RequestVerification {
                request_id,
                user_id,
            } => self.request_verification(request_id, user_id).await,
            ToActor::AcceptVerification {
                request_id,
                user_id,
                flow_id,
            } => self.accept_verification(request_id, user_id, flow_id).await,
            ToActor::ConfirmVerification {
                request_id,
                user_id: _,
                flow_id,
                emojis_match,
            } => {
                self.confirm_verification(request_id, flow_id, emojis_match)
                    .await
            }
            ToActor::CancelVerification {
                request_id,
                user_id: _,
                flow_id,
            } => self.cancel_verification(request_id, flow_id).await,
            ToActor::SetupRecovery {
                request_id,
                passphrase,
            } => self.setup_recovery(request_id, passphrase).await,
            ToActor::SubmitUiaResponse {
                request_id,
                session,
                password,
                passphrase,
            } => {
                self.submit_uia_response(request_id, session, password, passphrase)
                    .await
            }
            ToActor::RecoverIdentity {
                request_id,
                passphrase,
            } => self.recover_identity(request_id, passphrase).await,
            ToActor::EnableKeyBackup {
                request_id,
                passphrase: _,
            } => self.enable_key_backup(request_id).await,
            ToActor::RestoreKeyBackup {
                request_id,
                passphrase,
            } => self.restore_key_backup(request_id, passphrase).await,
            ToActor::RetryDecryption {
                request_id,
                room_id,
                session_id,
            } => self.retry_decryption(request_id, room_id, session_id).await,
            ToActor::ExportKeys {
                request_id,
                passphrase: _,
            } => self.export_keys(request_id),
            ToActor::ImportKeys {
                request_id,
                passphrase: _,
                payload: _,
            } => self.import_keys(request_id),
            ToActor::GenerateQrCode {
                request_id,
                user_id,
                flow_id,
            } => self.generate_qr_code(request_id, user_id, flow_id).await,
            ToActor::ConfirmQrScan {
                request_id,
                user_id,
                flow_id,
                scanned_data,
            } => {
                self.confirm_qr_scan(request_id, user_id, flow_id, scanned_data)
                    .await
            }
            ToActor::GetMyDevices { request_id } => self.get_my_devices(request_id).await,
            ToActor::DeleteDevice {
                request_id,
                device_id,
                uia_session,
                password,
            } => {
                self.delete_device(request_id, device_id, uia_session, password)
                    .await
            }
            ToActor::RequestRoomKey {
                request_id,
                room_id,
                session_id,
                sender_key,
            } => {
                self.request_room_key(request_id, room_id, session_id, sender_key)
                    .await
            }
            ToActor::ClearRoomWarning {
                request_id,
                room_id,
                user_id,
            } => self.clear_room_warning(request_id, room_id, user_id).await,
        }
    }
}
