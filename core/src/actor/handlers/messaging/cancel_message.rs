use crate::actor::MatrixActor;
use crate::actor::queue::TaskPayload;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::messaging::cancel_message::CancelMessageArgs;

pub async fn run(actor: &MatrixActor, args: CancelMessageArgs) -> Vec<ToShell> {
    let mut removed = false;

    let mut mgr = actor.queue_manager.lock().await;
    if let Some(state) = mgr.queues.get_mut(&args.room_id) {
        if let Some(pos) = state.tasks.iter().position(|t| {
            if let TaskPayload::SendMessage { txn_id, .. } = &t.payload {
                txn_id == &args.transaction_id
            } else {
                false
            }
        }) {
            let removed_task = state.tasks.remove(pos).unwrap();
            mgr.remove_from_db(&removed_task.id).await;
            removed = true;
        }
    }
    drop(mgr);

    vec![ToShell::Core(CoreEvents::CommandResult(
        CommandResultArgs {
            request_id: args.request_id,
            success: removed,
            error: None,
        },
    ))]
}
