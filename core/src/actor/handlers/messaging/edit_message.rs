use crate::actor::MatrixActor;
use matrix_sdk::ruma::events::room::message::{ReplacementMetadata, RoomMessageEventContent};
use pulldown_cmark::{Options, Parser, html};

use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::messaging::edit_message::EditMessageArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: EditMessageArgs) -> Vec<ToShell> {
    let client_opt = actor.client.borrow().clone();

    if client_opt.is_none()
        || client_opt
            .as_ref()
            .unwrap()
            .get_room(&args.room_id)
            .is_none()
    {
        return vec![ToShell::Core(CoreEvents::CommandResult(
            CommandResultArgs {
                request_id: args.request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            },
        ))];
    }

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(&args.new_body, options);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);

    let safe_html = selvedge_shared::sanitize_matrix_html(&raw_html);
    let trimmed_html = safe_html.trim().to_string();

    let ruma_content =
        RoomMessageEventContent::text_html(args.new_body.clone(), trimmed_html.clone());

    let replacement =
        ruma_content.make_replacement(ReplacementMetadata::new(args.event_id.clone(), None));

    let txn_id = matrix_sdk::ruma::TransactionId::new();

    // Reusing the SendMessage payload since an edit is just a message event
    let task_payload = crate::actor::queue::TaskPayload::SendMessage {
        txn_id: txn_id.clone(),
        content: replacement,
    };

    let task = crate::actor::queue::OutboundTask {
        id: uuid::Uuid::new_v4().to_string(),
        room_id: args.room_id.clone(),
        payload: task_payload,
    };

    let queue = actor.queue_manager.clone();
    let q_room_id = args.room_id.clone();

    wasm_bindgen_futures::spawn_local(async move {
        queue.lock().await.enqueue(task).await;
        crate::actor::queue::QueueManager::poke(queue, &q_room_id);
    });

    vec![ToShell::Core(CoreEvents::CommandResult(
        CommandResultArgs {
            request_id: args.request_id,
            success: true,
            error: None,
        },
    ))]
}
