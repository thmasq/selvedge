use super::super::MatrixActor;
use matrix_sdk::Client;
use selvedge_shared::{ActorError, message::ToShell};

impl MatrixActor {
    #[allow(clippy::future_not_send)]
    pub(crate) async fn login(&self, url: String, user: String, pass: String) -> Vec<ToShell> {
        let client_builder = Client::builder()
            .homeserver_url(&url)
            .indexeddb_store("selvedge-store", None);

        match client_builder.build().await {
            Ok(client) => match client.matrix_auth().login_username(&user, &pass).await {
                Ok(_) => {
                    *self.client.borrow_mut() = Some(client);
                    vec![ToShell::LoginSuccess]
                }
                Err(e) => vec![ToShell::LoginFailure(ActorError::LoginFailed(
                    e.to_string(),
                ))],
            },
            Err(e) => vec![ToShell::LoginFailure(ActorError::LoginFailed(
                e.to_string(),
            ))],
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn restore_session(&self) -> Vec<ToShell> {
        let client_builder = Client::builder().indexeddb_store("selvedge-store", None);

        match client_builder.build().await {
            Ok(client) => {
                if client.session_meta().is_some() {
                    *self.client.borrow_mut() = Some(client);
                    vec![ToShell::LoginSuccess]
                } else {
                    vec![ToShell::LoginFailure(ActorError::LoginFailed(
                        "No saved session found".to_string(),
                    ))]
                }
            }
            Err(e) => vec![ToShell::LoginFailure(ActorError::LoginFailed(
                e.to_string(),
            ))],
        }
    }
}
