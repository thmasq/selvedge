use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::room::leave_room::LeaveRoomArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: LeaveRoomArgs) -> Vec<ToShell> {
    let room = actor
        .client
        .borrow()
        .as_ref()
        .and_then(|c| c.get_room(&args.room_id));

    if let Some(room) = room {
        match room.leave().await {
            Ok(_) => {
                actor.active_timelines.borrow_mut().remove(&args.room_id);
                vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: true,
                        error: None,
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
