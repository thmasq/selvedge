use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::crypto::CryptoEvents;
use selvedge_shared::event::crypto::uiaa_prompt::UiaaPromptArgs;
use selvedge_shared::message::crypto::delete_device::DeleteDeviceArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: DeleteDeviceArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        let auth_data = if let (Some(session), Some(pass)) = (args.uia_session, args.password) {
            let identifier = matrix_sdk::ruma::api::client::uiaa::UserIdentifier::UserIdOrLocalpart(
                client
                    .user_id()
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
            );
            let mut uiaa_password =
                matrix_sdk::ruma::api::client::uiaa::Password::new(identifier, pass);
            uiaa_password.session = Some(session);
            Some(matrix_sdk::ruma::api::client::uiaa::AuthData::Password(
                uiaa_password,
            ))
        } else {
            None
        };

        let device_id_owned = matrix_sdk::ruma::OwnedDeviceId::from(args.device_id);
        let mut request =
            matrix_sdk::ruma::api::client::device::delete_devices::v3::Request::new(vec![
                device_id_owned,
            ]);

        if let Some(auth) = auth_data {
            request.auth = Some(auth);
        }

        match client.send(request).await {
            Ok(_) => vec![ToShell::Core(CoreEvents::CommandResult(
                CommandResultArgs {
                    request_id: args.request_id,
                    success: true,
                    error: None,
                },
            ))],
            Err(e) => {
                if let Some(uiaa_info) = e.as_uiaa_response() {
                    if let Some(session) = &uiaa_info.session {
                        return vec![ToShell::Crypto(CryptoEvents::UiaaPrompt(UiaaPromptArgs {
                            request_id: args.request_id,
                            session: session.clone(),
                        }))];
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
