use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::room::RoomEvents;
use selvedge_shared::event::room::room_members_searched::RoomMembersSearchedArgs;
use selvedge_shared::message::room::search_room_members::SearchRoomMembersArgs;
use selvedge_shared::model::{ActorError, MemberProfile};

pub async fn run(actor: &MatrixActor, args: SearchRoomMembersArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();

    if let Some(client) = client {
        if let Some(room) = client.get_room(&args.room_id) {
            let mut matches = Vec::new();
            let query = args.query.to_lowercase();

            if let Ok(members) = room.members(matrix_sdk::RoomMemberships::ACTIVE).await {
                for member in members {
                    let display_name = member.display_name().unwrap_or("").to_lowercase();
                    let user_id = member.user_id().as_str().to_lowercase();

                    if display_name.contains(&query) || user_id.contains(&query) {
                        matches.push(MemberProfile {
                            user_id: member.user_id().to_owned(),
                            display_name: member.display_name().map(ToOwned::to_owned),
                            avatar_url: member.avatar_url().map(ToOwned::to_owned),
                            membership: member.membership().clone(),
                            presence: matrix_sdk::ruma::presence::PresenceState::Offline,
                            is_verified: false,
                        });

                        if matches.len() >= args.limit {
                            break;
                        }
                    }
                }
            }

            vec![ToShell::Room(RoomEvents::RoomMembersSearched(
                RoomMembersSearchedArgs {
                    request_id: args.request_id,
                    room_id: args.room_id,
                    query: args.query,
                    results: matches,
                },
            ))]
        } else {
            vec![ToShell::Core(CoreEvents::CommandResult(
                CommandResultArgs {
                    request_id: args.request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed("Room not found".into())),
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
