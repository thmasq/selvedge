use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::confirm_verification::ConfirmVerificationArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: ConfirmVerificationArgs) -> Vec<ToShell> {
    let verification = actor
        .active_sas_verifications
        .borrow()
        .get(&args.flow_id)
        .cloned();

    if let Some(sas) = verification {
        let res = if args.emojis_match {
            sas.confirm().await
        } else {
            sas.mismatch().await
        };

        match res {
            Ok(()) => vec![ToShell::Core(CoreEvents::CommandResult(
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
                error: Some(ActorError::RoomOperationFailed(
                    "Verification flow not found".into(),
                )),
            },
        ))]
    }
}
