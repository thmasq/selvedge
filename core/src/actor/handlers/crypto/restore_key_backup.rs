use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::restore_key_backup::RestoreKeyBackupArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: RestoreKeyBackupArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        match client
            .encryption()
            .recovery()
            .recover(&args.passphrase)
            .await
        {
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
