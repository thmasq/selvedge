use crate::actor::MatrixActor;
use matrix_sdk::Client;
use matrix_sdk::cross_process_lock::CrossProcessLockConfig;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::auth::AuthEvents;
use selvedge_shared::event::auth::login_failure::LoginFailureArgs;
use selvedge_shared::event::auth::login_success::LoginSuccessArgs;
use selvedge_shared::message::auth::restore_session::RestoreSessionArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, _args: RestoreSessionArgs) -> Vec<ToShell> {
    let client_builder = Client::builder()
        .indexeddb_store("selvedge-store", None)
        .cross_process_store_config(CrossProcessLockConfig::multi_process("selvedge_app"));

    match client_builder.build().await {
        Ok(client) => {
            if client.session_meta().is_some() {
                *actor.client.borrow_mut() = Some(client);
                vec![ToShell::Auth(AuthEvents::LoginSuccess(LoginSuccessArgs))]
            } else {
                vec![ToShell::Auth(AuthEvents::LoginFailure(LoginFailureArgs {
                    error: ActorError::LoginFailed("No saved session found".to_string()),
                }))]
            }
        }
        Err(e) => vec![ToShell::Auth(AuthEvents::LoginFailure(LoginFailureArgs {
            error: ActorError::LoginFailed(e.to_string()),
        }))],
    }
}
