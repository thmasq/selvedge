use crate::actor::MatrixActor;
use matrix_sdk::ruma::EventEncryptionAlgorithm;
use matrix_sdk::ruma::api::client::to_device::send_event_to_device::v3::Request as ToDeviceRequest;
use matrix_sdk::ruma::events::AnyToDeviceEventContent;
use matrix_sdk::ruma::events::room_key_request::{
    Action, RequestedKeyInfo, ToDeviceRoomKeyRequestEventContent,
};
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::request_room_key::RequestRoomKeyArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: RequestRoomKeyArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        if let (Some(user_id), Some(current_device)) = (client.user_id(), client.device_id()) {
            if let Ok(devices) = client.encryption().get_user_devices(user_id).await {
                let mut target_devices = Vec::new();

                for device in devices.devices() {
                    if device.device_id() != current_device && device.is_cross_signed_by_owner() {
                        target_devices.push(device.device_id().to_owned());
                    }
                }

                if !target_devices.is_empty() {
                    let body = RequestedKeyInfo::new(
                        EventEncryptionAlgorithm::MegolmV1AesSha2,
                        args.room_id,
                        args.sender_key,
                        args.session_id,
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
                                    matrix_sdk::ruma::to_device::DeviceIdOrAllDevices::DeviceId(
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
                                Ok(_) => vec![ToShell::Core(CoreEvents::CommandResult(
                                    CommandResultArgs {
                                        request_id: args.request_id,
                                        success: true,
                                        error: None,
                                    },
                                ))],
                                Err(e) => vec![ToShell::Core(CoreEvents::CommandResult(
                                    CommandResultArgs {
                                        request_id: args.request_id,
                                        success: false,
                                        error: Some(ActorError::RoomOperationFailed(e.to_string())),
                                    },
                                ))],
                            }
                        }
                        Err(e) => vec![ToShell::Core(CoreEvents::CommandResult(
                            CommandResultArgs {
                                request_id: args.request_id,
                                success: false,
                                error: Some(ActorError::RoomOperationFailed(e.to_string())),
                            },
                        ))],
                    }
                } else {
                    vec![ToShell::Core(CoreEvents::CommandResult(
                        CommandResultArgs {
                            request_id: args.request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(
                                "No other verified devices found to request keys from.".into(),
                            )),
                        },
                    ))]
                }
            } else {
                vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(
                            "Failed to fetch user devices".into(),
                        )),
                    },
                ))]
            }
        } else {
            vec![ToShell::Core(CoreEvents::CommandResult(
                CommandResultArgs {
                    request_id: args.request_id,
                    success: false,
                    error: Some(ActorError::ClientNotInitialized),
                },
            ))]
        }
    } else {
        vec![ToShell::Core(CoreEvents::CommandResult(
            CommandResultArgs {
                request_id: args.request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            },
        ))]
    }
}
