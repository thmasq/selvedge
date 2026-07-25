use crate::actor::MatrixActor;
use matrix_sdk::attachment::AttachmentConfig;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::messaging::send_media::SendMediaArgs;
use selvedge_shared::model::ActorError;
use std::str::FromStr;

pub async fn run(actor: &MatrixActor, args: SendMediaArgs) -> Vec<ToShell> {
    let room = actor
        .client
        .borrow()
        .as_ref()
        .and_then(|c| c.get_room(&args.room_id));

    let result = if let Some(room) = room {
        let config = AttachmentConfig::new();
        if let Ok(mime) = mime::Mime::from_str(&args.mime_type) {
            room.send_attachment(&args.filename, &mime, args.data, config)
                .await
                .map(|_| ())
                .map_err(|e| ActorError::RoomOperationFailed(e.to_string()))
        } else {
            Err(ActorError::RoomOperationFailed(
                "Invalid MIME type".to_string(),
            ))
        }
    } else {
        Err(ActorError::ClientNotInitialized)
    };

    match result {
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
                error: Some(e),
            },
        ))],
    }
}
