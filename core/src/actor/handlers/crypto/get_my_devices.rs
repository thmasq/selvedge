use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::crypto::CryptoEvents;
use selvedge_shared::event::crypto::device_list_result::DeviceListResultArgs;
use selvedge_shared::message::crypto::get_my_devices::GetMyDevicesArgs;
use selvedge_shared::model::{ActorError, DeviceInfo};

pub async fn run(actor: &MatrixActor, args: GetMyDevicesArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
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

                vec![ToShell::Crypto(CryptoEvents::DeviceListResult(
                    DeviceListResultArgs {
                        request_id: args.request_id,
                        devices: device_infos,
                    },
                ))]
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
                error: Some(ActorError::ClientNotInitialized),
            },
        ))]
    }
}
