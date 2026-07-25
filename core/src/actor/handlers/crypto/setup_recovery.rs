use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::crypto::CryptoEvents;
use selvedge_shared::event::crypto::uiaa_prompt::UiaaPromptArgs;
use selvedge_shared::message::crypto::setup_recovery::SetupRecoveryArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: SetupRecoveryArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        match client
            .encryption()
            .recovery()
            .enable()
            .with_passphrase(&args.passphrase)
            .await
        {
            Ok(_) => {
                let _ = client.encryption().backups().wait_for_steady_state().await;
                vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: true,
                        error: None,
                    },
                ))]
            }
            Err(e) => {
                if let matrix_sdk::encryption::recovery::RecoveryError::Sdk(sdk_err) = &e {
                    if let Some(uiaa_info) = sdk_err.as_uiaa_response() {
                        if let Some(session) = &uiaa_info.session {
                            return vec![ToShell::Crypto(CryptoEvents::UiaaPrompt(
                                UiaaPromptArgs {
                                    request_id: args.request_id,
                                    session: session.clone(),
                                },
                            ))];
                        }
                    }
                }
                vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(e.to_string())),
                    },
                ))]
            }
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
