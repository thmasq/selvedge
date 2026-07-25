use crate::actor::MatrixActor;
use matrix_sdk::Client;
use selvedge_shared::event::ToShell;
use selvedge_shared::message::auth::login::LoginArgs;
use selvedge_shared::model::ActorError;

use selvedge_shared::event::auth::AuthEvents;
use selvedge_shared::event::auth::login_failure::LoginFailureArgs;
use selvedge_shared::event::auth::login_success::LoginSuccessArgs;

pub async fn run(actor: &MatrixActor, args: LoginArgs) -> Vec<ToShell> {
    let client_builder = Client::builder()
        .homeserver_url(&args.homeserver_url)
        .indexeddb_store("selvedge-store", None);

    match client_builder.build().await {
        Ok(client) => match client
            .matrix_auth()
            .login_username(&args.username, &args.password)
            .await
        {
            Ok(_) => {
                *actor.client.borrow_mut() = Some(client);

                vec![ToShell::Auth(AuthEvents::LoginSuccess(LoginSuccessArgs))]
            }
            Err(e) => {
                vec![ToShell::Auth(AuthEvents::LoginFailure(LoginFailureArgs {
                    error: ActorError::LoginFailed(e.to_string()),
                }))]
            }
        },
        Err(e) => {
            vec![ToShell::Auth(AuthEvents::LoginFailure(LoginFailureArgs {
                error: ActorError::LoginFailed(e.to_string()),
            }))]
        }
    }
}
