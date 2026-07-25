use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::retry_decryption::RetryDecryptionArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: RetryDecryptionArgs) -> Vec<ToShell> {
    let timeline = actor.active_timelines.borrow().get(&args.room_id).cloned();
    if let Some(timeline) = timeline {
        timeline
            .retry_decryption(std::iter::once(args.session_id.as_str()))
            .await;
        vec![ToShell::Core(CoreEvents::CommandResult(
            CommandResultArgs {
                request_id: args.request_id,
                success: true,
                error: None,
            },
        ))]
    } else {
        vec![ToShell::Core(CoreEvents::CommandResult(
            CommandResultArgs {
                request_id: args.request_id,
                success: false,
                error: Some(ActorError::RoomOperationFailed(
                    "Timeline not found for the given room".into(),
                )),
            },
        ))]
    }
}
