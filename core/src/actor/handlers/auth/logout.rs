use crate::actor::MatrixActor;
use gloo_storage::Storage;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::auth::logout::LogoutArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: LogoutArgs) -> Vec<ToShell> {
    let client_opt = actor.client.borrow().clone();

    if let Some(client) = client_opt {
        let _ = client.encryption().backups().wait_for_steady_state().await;

        match client.matrix_auth().logout().await {
            Ok(_) => {
                *actor.client.borrow_mut() = None;

                actor.active_timelines.borrow_mut().clear();
                actor.active_sas_verifications.borrow_mut().clear();
                actor.active_qr_verifications.borrow_mut().clear();

                *actor.search_index.borrow_mut() = crate::actor::search::SearchIndex::default();
                let _ = gloo_storage::LocalStorage::clear();

                if let Some(window) = web_sys::window() {
                    if let Ok(Some(idb)) = window.indexed_db() {
                        let _ = idb.delete_database("matrix-sdk-crypto");
                        let _ = idb.delete_database("matrix-sdk-state");
                    }
                }

                vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: true,
                        error: None,
                    },
                ))]
            }
            Err(e) => {
                vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: false,
                        error: Some(ActorError::LogoutFailed(e.to_string())),
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
