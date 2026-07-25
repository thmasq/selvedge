use crate::actor::MatrixActor;
use futures::StreamExt;
use matrix_sdk::RoomMemberships;
use matrix_sdk::ruma::presence::PresenceState;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::room::RoomEvents;
use selvedge_shared::event::room::room_members_loaded::RoomMembersLoadedArgs;
use selvedge_shared::message::room::load_room_members::LoadRoomMembersArgs;
use selvedge_shared::model::{ActorError, MemberProfile};
use std::collections::HashMap;

pub async fn run(actor: &MatrixActor, args: LoadRoomMembersArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();

    if let Some(client) = client {
        if let Some(room) = client.get_room(&args.room_id) {
            match room.members(RoomMemberships::ACTIVE).await {
                Ok(sdk_members) => {
                    let members_stream = futures::stream::iter(sdk_members)
                        .map(|member| {
                            let client_ref = client.clone();
                            async move {
                                let user_id = member.user_id().to_owned();
                                let is_verified = client_ref
                                    .encryption()
                                    .get_user_identity(&user_id)
                                    .await
                                    .ok()
                                    .flatten()
                                    .is_some_and(|id| id.is_verified());
                                let mut presence = PresenceState::Offline;

                                if let Ok(Some(raw_ev)) =
                                    client_ref.state_store().get_presence_event(&user_id).await
                                {
                                    let deserialized: Result<
                                        matrix_sdk::ruma::events::presence::PresenceEvent,
                                        _,
                                    > = raw_ev.deserialize();
                                    if let Ok(ev) = deserialized {
                                        presence = match ev.content.presence {
                                            PresenceState::Online => PresenceState::Online,
                                            PresenceState::Unavailable => {
                                                PresenceState::Unavailable
                                            }
                                            PresenceState::Offline => PresenceState::Offline,
                                            _ => PresenceState::Unavailable,
                                        };
                                    }
                                }

                                (
                                    user_id.clone(),
                                    MemberProfile {
                                        user_id,
                                        display_name: member.display_name().map(ToOwned::to_owned),
                                        avatar_url: member.avatar_url().map(ToOwned::to_owned),
                                        membership: member.membership().clone(),
                                        presence,
                                        is_verified,
                                    },
                                )
                            }
                        })
                        .buffer_unordered(50);

                    let members_list: Vec<_> = members_stream.collect().await;
                    let members: HashMap<_, _> = members_list.into_iter().collect();

                    vec![ToShell::Room(RoomEvents::RoomMembersLoaded(
                        RoomMembersLoadedArgs {
                            request_id: args.request_id,
                            room_id: args.room_id,
                            members,
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
                    error: Some(ActorError::RoomOperationFailed(
                        "Room not found".to_string(),
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
}
