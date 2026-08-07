use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::messaging::retry_message::RetryMessageArgs;

pub async fn run(actor: &MatrixActor, args: RetryMessageArgs) -> Vec<ToShell> {
    {
        let mut mgr = actor.queue_manager.lock().await;
        if let Some(state) = mgr.queues.get_mut(&args.room_id) {
            state.failures = 0;
            state.backoff_until = None;
        }
    }

    crate::actor::queue::QueueManager::poke(actor.queue_manager.clone(), &args.room_id);

    vec![ToShell::Core(CoreEvents::CommandResult(
        CommandResultArgs {
            request_id: args.request_id,
            success: true,
            error: None,
        },
    ))]
}
