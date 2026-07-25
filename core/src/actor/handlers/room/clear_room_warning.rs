use crate::actor::MatrixActor;
use gloo_storage::{LocalStorage, Storage};
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::room::RoomEvents;
use selvedge_shared::event::room::room_trust_level_updated::RoomTrustLevelUpdatedArgs;
use selvedge_shared::message::room::clear_room_warning::ClearRoomWarningArgs;
use selvedge_shared::model::RoomTrustLevel;

pub async fn run(actor: &MatrixActor, args: ClearRoomWarningArgs) -> Vec<ToShell> {
    let storage_key = format!("trust_state_{}", args.user_id);
    let _ = LocalStorage::set(&storage_key, false);

    let mut responses = vec![ToShell::Core(CoreEvents::CommandResult(
        CommandResultArgs {
            request_id: args.request_id.clone(),
            success: true,
            error: None,
        },
    ))];

    let client = actor.client.borrow().clone();
    if let Some(client) = client
        && let Some(room) = client.get_room(&args.room_id) {
            let is_encrypted = room.encryption_state().is_encrypted();
            let mut trust_level = if is_encrypted {
                RoomTrustLevel::Trusted
            } else {
                RoomTrustLevel::Plain
            };

            if is_encrypted
                && let Ok(members) = room.members(matrix_sdk::RoomMemberships::ACTIVE).await {
                    for member in members {
                        let m_user_id = member.user_id().to_owned();
                        let is_user_verified = client
                            .encryption()
                            .get_user_identity(&m_user_id)
                            .await
                            .ok()
                            .flatten()
                            .is_some_and(|identity| identity.is_verified());
                        let m_storage_key = format!("trust_state_{m_user_id}");
                        let prev_verified: Option<bool> = LocalStorage::get(&m_storage_key).ok();

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

            responses.push(ToShell::Room(RoomEvents::RoomTrustLevelUpdated(
                RoomTrustLevelUpdatedArgs {
                    room_id: args.room_id,
                    trust_level,
                },
            )));
        }

    responses
}
