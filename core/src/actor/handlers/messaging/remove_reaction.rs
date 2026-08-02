use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::messaging::remove_reaction::RemoveReactionArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: RemoveReactionArgs) -> Vec<ToShell> {
    let client_opt = actor.client.borrow().clone();

    if client_opt.is_none()
        || client_opt
            .as_ref()
            .unwrap()
            .get_room(&args.room_id)
            .is_none()
    {
        return vec![ToShell::Core(CoreEvents::CommandResult(
            CommandResultArgs {
                request_id: args.request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            },
        ))];
    }

    let task = crate::actor::queue::OutboundTask {
        id: uuid::Uuid::new_v4().to_string(),
        room_id: args.room_id.clone(),
        payload: crate::actor::queue::TaskPayload::RemoveReaction {
            target_event_id: args.target_event_id.clone(),
            reaction_event_id: args.reaction_event_id.clone(),
            key: args.key.clone(),
        },
    };

    let queue = actor.queue_manager.clone();
    let q_room_id = args.room_id.clone();

    wasm_bindgen_futures::spawn_local(async move {
        queue.lock().await.enqueue(task).await;
        crate::actor::queue::QueueManager::poke(queue, &q_room_id);
    });

    let local_echo_event = ToShell::Room(selvedge_shared::event::room::RoomEvents::TimelineDiff(
        selvedge_shared::event::room::timeline_diff::TimelineDiffArgs {
            room_id: args.room_id.clone(),
            diff: vec![selvedge_shared::model::TimelineDiff::UpdateReaction {
                event_id: args.target_event_id.clone(),
                key: args.key.clone(),
                delta: -1,
            }],
        },
    ));

    vec![
        local_echo_event,
        ToShell::Core(CoreEvents::CommandResult(CommandResultArgs {
            request_id: args.request_id,
            success: true,
            error: None,
        })),
    ]
}
