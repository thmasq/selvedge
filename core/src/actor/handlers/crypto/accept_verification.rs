use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::accept_verification::AcceptVerificationArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: AcceptVerificationArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        if let Some(request) = client
            .encryption()
            .get_verification_request(&args.user_id, &args.flow_id)
            .await
        {
            match request.accept().await {
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
        } else if let Some(sas) = actor
            .active_sas_verifications
            .borrow()
            .get(&args.flow_id)
            .cloned()
        {
            match sas.accept().await {
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
                    error: Some(ActorError::RoomOperationFailed(
                        "Verification flow not found".into(),
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
