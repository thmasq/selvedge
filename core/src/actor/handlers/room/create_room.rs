use crate::actor::MatrixActor;
use matrix_sdk::ruma::EventEncryptionAlgorithm;
use matrix_sdk::ruma::events::AnyInitialStateEvent;
use matrix_sdk::ruma::events::room::encryption::RoomEncryptionEventContent;
use matrix_sdk::ruma::serde::Raw;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::room::create_room::CreateRoomArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: CreateRoomArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        let mut request = matrix_sdk::ruma::api::client::room::create_room::v3::Request::new();
        request.name = Some(args.name);
        request.topic = args.topic;

        if args.is_encrypted {
            let content =
                RoomEncryptionEventContent::new(EventEncryptionAlgorithm::MegolmV1AesSha2);
            let raw_event = serde_json::json!({ "type": "m.room.encryption", "state_key": "", "content": content });

            if let Ok(raw_initial_state) =
                serde_json::from_value::<Raw<AnyInitialStateEvent>>(raw_event)
            {
                request.initial_state.push(raw_initial_state);
            }
        }

        match client.create_room(request).await {
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
                error: Some(ActorError::ClientNotInitialized),
            },
        ))]
    }
}
