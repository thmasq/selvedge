use super::super::MatrixActor;
use gloo_storage::{LocalStorage, Storage};
use matrix_sdk::ruma::api::client::to_device::send_event_to_device::v3::Request as ToDeviceRequest;
use matrix_sdk::ruma::events::AnyToDeviceEventContent;
use matrix_sdk::ruma::events::room_key_request::{
    Action, RequestedKeyInfo, ToDeviceRoomKeyRequestEventContent,
};
use matrix_sdk::ruma::{EventEncryptionAlgorithm, OwnedRoomId, OwnedUserId};
use selvedge_shared::{ActorError, DeviceInfo, RoomTrustLevel, message::ToShell};

impl MatrixActor {
    #[allow(clippy::future_not_send)]
    pub(crate) async fn request_verification(
        &self,
        request_id: String,
        user_id: OwnedUserId,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            match client.encryption().request_user_identity(&user_id).await {
                Ok(Some(user_identity)) => match user_identity.request_verification().await {
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
                },
                Ok(None) => vec![ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(
                        "User identity not found on the server".into(),
                    )),
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
    pub(crate) async fn accept_verification(
        &self,
        request_id: String,
        user_id: OwnedUserId,
        flow_id: String,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            if let Some(request) = client
                .encryption()
                .get_verification_request(&user_id, &flow_id)
                .await
            {
                match request.accept().await {
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
            } else if let Some(sas) = self
                .active_sas_verifications
                .borrow()
                .get(&flow_id)
                .cloned()
            {
                match sas.accept().await {
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
                    error: Some(ActorError::RoomOperationFailed(
                        "Verification flow not found".into(),
                    )),
                }]
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
    pub(crate) async fn confirm_verification(
        &self,
        request_id: String,
        flow_id: String,
        emojis_match: bool,
    ) -> Vec<ToShell> {
        let verification = self
            .active_sas_verifications
            .borrow()
            .get(&flow_id)
            .cloned();

        if let Some(sas) = verification {
            let res = if emojis_match {
                sas.confirm().await
            } else {
                sas.mismatch().await
            };

            match res {
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
                error: Some(ActorError::RoomOperationFailed(
                    "Verification flow not found".into(),
                )),
            }]
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn cancel_verification(
        &self,
        request_id: String,
        flow_id: String,
    ) -> Vec<ToShell> {
        let verification = self
            .active_sas_verifications
            .borrow()
            .get(&flow_id)
            .cloned();

        if let Some(sas) = verification {
            match sas.cancel().await {
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
                error: Some(ActorError::RoomOperationFailed(
                    "Verification flow not found".into(),
                )),
            }]
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn setup_recovery(
        &self,
        request_id: String,
        passphrase: String,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            match client
                .encryption()
                .recovery()
                .enable()
                .with_passphrase(&passphrase)
                .await
            {
                Ok(_) => {
                    let _ = client.encryption().backups().wait_for_steady_state().await;
                    vec![ToShell::CommandResult {
                        request_id,
                        success: true,
                        error: None,
                    }]
                }
                Err(e) => {
                    if let matrix_sdk::encryption::recovery::RecoveryError::Sdk(sdk_err) = &e {
                        if let Some(uiaa_info) = sdk_err.as_uiaa_response() {
                            if let Some(session) = &uiaa_info.session {
                                return vec![ToShell::UiaaPrompt {
                                    request_id,
                                    session: session.clone(),
                                }];
                            }
                        }
                    }

                    vec![ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(e.to_string())),
                    }]
                }
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
    pub(crate) async fn submit_uia_response(
        &self,
        request_id: String,
        session: String,
        password: String,
        passphrase: String,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            let identifier = matrix_sdk::ruma::api::client::uiaa::UserIdentifier::UserIdOrLocalpart(
                client
                    .user_id()
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            );

            let mut uiaa_password =
                matrix_sdk::ruma::api::client::uiaa::Password::new(identifier, password);
            uiaa_password.session = Some(session);

            let auth_data = matrix_sdk::ruma::api::client::uiaa::AuthData::Password(uiaa_password);

            match client
                .encryption()
                .bootstrap_cross_signing(Some(auth_data))
                .await
            {
                Ok(_) => match client
                    .encryption()
                    .recovery()
                    .enable()
                    .with_passphrase(&passphrase)
                    .await
                {
                    Ok(_) => {
                        let _ = client.encryption().backups().wait_for_steady_state().await;
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
                },
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
    pub(crate) async fn recover_identity(
        &self,
        request_id: String,
        passphrase: String,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            match client.encryption().recovery().recover(&passphrase).await {
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
    pub(crate) async fn enable_key_backup(&self, request_id: String) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            match client.encryption().backups().create().await {
                Ok(_) => {
                    let _ = client.encryption().backups().wait_for_steady_state().await;
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
    pub(crate) async fn restore_key_backup(
        &self,
        request_id: String,
        passphrase: String,
    ) -> Vec<ToShell> {
        // Implementation is identical to recover_identity
        self.recover_identity(request_id, passphrase).await
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn retry_decryption(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
        session_id: String,
    ) -> Vec<ToShell> {
        let timeline = self.active_timelines.borrow().get(&room_id).cloned();
        if let Some(timeline) = timeline {
            timeline
                .retry_decryption(std::iter::once(session_id.as_str()))
                .await;

            vec![ToShell::CommandResult {
                request_id,
                success: true,
                error: None,
            }]
        } else {
            vec![ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(ActorError::RoomOperationFailed(
                    "Timeline not found for the given room".into(),
                )),
            }]
        }
    }

    pub(crate) fn export_keys(&self, request_id: String) -> Vec<ToShell> {
        vec![ToShell::CommandResult {
            request_id,
            success: false,
            error: Some(ActorError::RoomOperationFailed(
                "Manual file export is not supported in the web environment. Please use Account Recovery / Key Backup instead.".into(),
            )),
        }]
    }

    pub(crate) fn import_keys(&self, request_id: String) -> Vec<ToShell> {
        vec![ToShell::CommandResult {
            request_id,
            success: false,
            error: Some(ActorError::RoomOperationFailed(
                "Manual file import is not supported in the web environment. Please use Account Recovery / Key Backup instead.".into(),
            )),
        }]
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn generate_qr_code(
        &self,
        request_id: String,
        user_id: OwnedUserId,
        flow_id: String,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
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
                        vec![ToShell::QrCodeGenerated {
                            request_id,
                            payload: bytes,
                        }]
                    }
                    Ok(None) => vec![ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(
                            "Could not generate QR code (not supported or invalid state)".into(),
                        )),
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
                    error: Some(ActorError::RoomOperationFailed(
                        "Verification flow not found".into(),
                    )),
                }]
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
    pub(crate) async fn confirm_qr_scan(
        &self,
        request_id: String,
        user_id: OwnedUserId,
        flow_id: String,
        scanned_data: Vec<u8>,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
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
                        },
                        Ok(None) => vec![ToShell::CommandResult {
                            request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(
                                "Scanned QR code does not match this verification flow".into(),
                            )),
                        }],
                        Err(e) => vec![ToShell::CommandResult {
                            request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(e.to_string())),
                        }],
                    },
                    Err(e) => vec![ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(format!(
                            "Invalid QR data: {}",
                            e
                        ))),
                    }],
                }
            } else {
                vec![ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(
                        "Verification flow not found".into(),
                    )),
                }]
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
    pub(crate) async fn get_my_devices(&self, request_id: String) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            match client.devices().await {
                Ok(response) => {
                    let mut device_infos = Vec::new();
                    let user_id = client.user_id().unwrap();
                    let current_device_id = client.device_id().unwrap();

                    let crypto_devices = client.encryption().get_user_devices(user_id).await.ok();

                    for device in response.devices {
                        let is_verified = if let Some(cd) = &crypto_devices {
                            cd.devices()
                                .find(|d| d.device_id() == device.device_id)
                                .map(|d| d.is_cross_signed_by_owner())
                                .unwrap_or(false)
                        } else {
                            false
                        };

                        device_infos.push(DeviceInfo {
                            device_id: device.device_id.to_string(),
                            display_name: device.display_name,
                            last_seen_ts: device.last_seen_ts.map(|ts| ts.0.into()),
                            last_seen_ip: device.last_seen_ip,
                            is_verified,
                            is_current_device: device.device_id == current_device_id,
                        });
                    }

                    vec![ToShell::DeviceListResult {
                        request_id,
                        devices: device_infos,
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
    pub(crate) async fn delete_device(
        &self,
        request_id: String,
        device_id: String,
        uia_session: Option<String>,
        password: Option<String>,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            let auth_data = if let (Some(session), Some(pass)) = (uia_session, password) {
                let identifier =
                    matrix_sdk::ruma::api::client::uiaa::UserIdentifier::UserIdOrLocalpart(
                        client
                            .user_id()
                            .map(|id| id.to_string())
                            .unwrap_or_default(),
                    );
                let mut uiaa_password =
                    matrix_sdk::ruma::api::client::uiaa::Password::new(identifier, pass);
                uiaa_password.session = Some(session);
                Some(matrix_sdk::ruma::api::client::uiaa::AuthData::Password(
                    uiaa_password,
                ))
            } else {
                None
            };

            let device_id_owned = matrix_sdk::ruma::OwnedDeviceId::from(device_id);
            let mut request =
                matrix_sdk::ruma::api::client::device::delete_devices::v3::Request::new(vec![
                    device_id_owned,
                ]);

            if let Some(auth) = auth_data {
                request.auth = Some(auth);
            }

            match client.send(request).await {
                Ok(_) => vec![ToShell::CommandResult {
                    request_id,
                    success: true,
                    error: None,
                }],
                Err(e) => {
                    if let Some(uiaa_info) = e.as_uiaa_response() {
                        if let Some(session) = &uiaa_info.session {
                            return vec![ToShell::UiaaPrompt {
                                request_id,
                                session: session.clone(),
                            }];
                        }
                    }

                    vec![ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(e.to_string())),
                    }]
                }
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
    pub(crate) async fn request_room_key(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
        session_id: String,
        sender_key: String,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            if let (Some(user_id), Some(current_device)) = (client.user_id(), client.device_id()) {
                if let Ok(devices) = client.encryption().get_user_devices(user_id).await {
                    let mut target_devices = Vec::new();

                    for device in devices.devices() {
                        if device.device_id() != current_device && device.is_cross_signed_by_owner()
                        {
                            target_devices.push(device.device_id().to_owned());
                        }
                    }

                    if !target_devices.is_empty() {
                        let body = RequestedKeyInfo::new(
                            EventEncryptionAlgorithm::MegolmV1AesSha2,
                            room_id,
                            sender_key,
                            session_id,
                        );

                        let txn_id = matrix_sdk::ruma::TransactionId::new();
                        let content = ToDeviceRoomKeyRequestEventContent::new(
                            Action::Request,
                            Some(body),
                            current_device.to_owned(),
                            txn_id.clone(),
                        );

                        match matrix_sdk::ruma::serde::Raw::new(&content) {
                            Ok(raw) => {
                                let raw_content = raw.cast::<AnyToDeviceEventContent>();

                                let mut device_map = std::collections::BTreeMap::new();
                                for target_device in target_devices {
                                    device_map.insert(
                                        ruma::to_device::DeviceIdOrAllDevices::DeviceId(
                                            target_device,
                                        ),
                                        raw_content.clone(),
                                    );
                                }

                                let mut messages = std::collections::BTreeMap::new();
                                messages.insert(user_id.to_owned(), device_map);

                                let request = ToDeviceRequest::new_raw(
                                    "m.room_key_request".into(),
                                    txn_id,
                                    messages,
                                );

                                match client.send(request).await {
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
                            error: Some(ActorError::RoomOperationFailed(
                                "No other verified devices found to request keys from.".into(),
                            )),
                        }]
                    }
                } else {
                    vec![ToShell::CommandResult {
                        request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(
                            "Failed to fetch user devices".into(),
                        )),
                    }]
                }
            } else {
                vec![ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::ClientNotInitialized),
                }]
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
    pub(crate) async fn clear_room_warning(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
        user_id: OwnedUserId,
    ) -> Vec<ToShell> {
        let storage_key = format!("trust_state_{}", user_id);
        let _ = LocalStorage::set(&storage_key, false);

        let mut responses = vec![ToShell::CommandResult {
            request_id,
            success: true,
            error: None,
        }];

        let client = self.client.borrow().clone();
        if let Some(client) = client {
            if let Some(room) = client.get_room(&room_id) {
                let is_encrypted = room.encryption_state().is_encrypted();

                let mut trust_level = if is_encrypted {
                    RoomTrustLevel::Trusted
                } else {
                    RoomTrustLevel::Plain
                };

                if is_encrypted {
                    if let Ok(members) = room.members(matrix_sdk::RoomMemberships::ACTIVE).await {
                        for member in members {
                            let m_user_id = member.user_id().to_owned();

                            let is_user_verified = client
                                .encryption()
                                .get_user_identity(&m_user_id)
                                .await
                                .ok()
                                .flatten()
                                .is_some_and(|identity| identity.is_verified());

                            let m_storage_key = format!("trust_state_{}", m_user_id);
                            let prev_verified: Option<bool> =
                                LocalStorage::get(&m_storage_key).ok();

                            if !is_user_verified {
                                if prev_verified == Some(true) {
                                    trust_level = RoomTrustLevel::Warning;
                                } else if trust_level == RoomTrustLevel::Trusted {
                                    trust_level = RoomTrustLevel::Normal;
                                }
                            } else if let Ok(devices) =
                                client.encryption().get_user_devices(&m_user_id).await
                            {
                                for device in devices.devices() {
                                    if !device.is_cross_signed_by_owner() {
                                        trust_level = RoomTrustLevel::Warning;
                                        break;
                                    }
                                }
                            }

                            if is_user_verified && prev_verified != Some(true) {
                                let _ = LocalStorage::set(&m_storage_key, true);
                            }
                        }
                    }
                }

                responses.push(ToShell::RoomTrustLevelUpdated {
                    room_id,
                    trust_level,
                });
            }
        }

        responses
    }
}
