use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::submit_uia_response::SubmitUiaResponseArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: SubmitUiaResponseArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        let identifier = matrix_sdk::ruma::api::client::uiaa::UserIdentifier::UserIdOrLocalpart(
            client
                .user_id()
                .map(|id| id.to_string())
                .unwrap_or_default(),
        );

        let mut uiaa_password =
            matrix_sdk::ruma::api::client::uiaa::Password::new(identifier, args.password);
        uiaa_password.session = Some(args.session);
        let auth_data = matrix_sdk::ruma::api::client::uiaa::AuthData::Password(uiaa_password);

        match client
            .encryption()
            .bootstrap_cross_signing(Some(auth_data))
            .await
        {
            Ok(_) => match client
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
                Err(e) => vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(e.to_string())),
                    },
                ))],
            },
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
