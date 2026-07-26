use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::cancel_verification::CancelVerificationArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: CancelVerificationArgs) -> Vec<ToShell> {
    let sas_verification = actor
        .active_sas_verifications
        .borrow()
        .get(&args.flow_id)
        .cloned();

    if let Some(sas) = sas_verification {
        match sas.cancel().await {
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
        let qr_verification = actor
            .active_qr_verifications
            .borrow()
            .get(&args.flow_id)
            .cloned();

        if let Some(qr) = qr_verification {
            match qr.cancel().await {
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
}
