use crate::model::{
    ActorError, DeviceInfo, EventItem, RoomDetails, RoomListEntryDiff, TimelineDiff,
    VerificationState,
};
use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId, OwnedUserId};
use serde::{Deserialize, Serialize};

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
    GetMyDevices {
        request_id: String,
    },
    DeleteDevice {
        request_id: String,
        device_id: String,
        uia_session: Option<String>,
        password: Option<String>,
    },
    RequestRoomKey {
        request_id: String,
        room_id: OwnedRoomId,
        session_id: String,
        sender_key: String,
    },
    SearchMessages {
        request_id: String,
        room_id: Option<OwnedRoomId>,
        query: String,
        limit: usize,
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
    DeviceListResult {
        request_id: String,
        devices: Vec<DeviceInfo>,
    },
    SearchResults {
        request_id: String,
        results: Vec<EventItem>,
    },
}
