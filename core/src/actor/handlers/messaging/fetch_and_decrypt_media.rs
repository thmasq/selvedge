use crate::actor::MatrixActor;
use matrix_sdk::media::{MediaFormat, MediaRequestParameters};
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::crypto::CryptoEvents;
use selvedge_shared::event::crypto::media_decrypted::MediaDecryptedArgs;
use selvedge_shared::message::messaging::fetch_and_decrypt_media::FetchAndDecryptMediaArgs;
use selvedge_shared::model::{ActorError, MediaSource};

pub async fn run(actor: &MatrixActor, args: FetchAndDecryptMediaArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();

    if let Some(client) = client {
        let ruma_source = match args.source {
            MediaSource::Plain(uri) => matrix_sdk::ruma::events::room::MediaSource::Plain(uri),
            MediaSource::Encrypted(file) => {
                matrix_sdk::ruma::events::room::MediaSource::Encrypted(Box::new(*file))
            }
        };

        let request = MediaRequestParameters {
            source: ruma_source,
            format: MediaFormat::File,
        };

        match client.media().get_media_content(&request, true).await {
            Ok(data) => vec![ToShell::Crypto(CryptoEvents::MediaDecrypted(
                MediaDecryptedArgs {
                    request_id: args.request_id,
                    mime_type: args.mime_type,
                    data,
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
                error: Some(ActorError::ClientNotInitialized),
            },
        ))]
    }
}
