use crate::actor::MatrixActor;
use matrix_sdk::ruma::events::room::message::{
    AddMentions, ForwardThread, RoomMessageEventContent,
};
use matrix_sdk::ruma::events::{AnyMessageLikeEvent, AnyTimelineEvent, MessageLikeEvent};
use pulldown_cmark::{Options, Parser, html};

use selvedge_shared::message::messaging::send_message::SendMessageArgs;

use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: SendMessageArgs) -> Vec<ToShell> {
    let timeline = actor.active_timelines.borrow().get(&args.room_id).cloned();

    let room = actor
        .client
        .borrow()
        .as_ref()
        .and_then(|c| c.get_room(&args.room_id));

    let result = if let Some(room) = room {
        let mut options = Options::empty();
        options.insert(Options::ENABLE_STRIKETHROUGH);
        options.insert(Options::ENABLE_TABLES);

        let parser = Parser::new_ext(&args.body, options);
        let mut raw_html = String::new();
        html::push_html(&mut raw_html, parser);

        let safe_html = selvedge_shared::sanitize_matrix_html(&raw_html);
        let trimmed_html = safe_html.trim().to_string();

        let mut content = RoomMessageEventContent::text_html(args.body.clone(), trimmed_html);

        if let Some(event_id) = args.reply_to
            && let Ok(event) = room.event(&event_id, None).await
                && let Ok(any_event) = event.kind.into_raw().deserialize() {
                    let full_event = any_event.into_full_event(args.room_id.clone());

                    if let AnyTimelineEvent::MessageLike(AnyMessageLikeEvent::RoomMessage(
                        MessageLikeEvent::Original(orig_msg),
                    )) = full_event
                    {
                        content =
                            content.make_reply_to(&orig_msg, ForwardThread::Yes, AddMentions::Yes);
                    }
                }

        if let Some(timeline) = timeline {
            timeline
                .send(content.into())
                .await
                .map(|_| ())
                .map_err(|_e| ActorError::RoomOperationFailed("Timeline send failed".to_string()))
        } else {
            room.send(content)
                .await
                .map(|_| ())
                .map_err(|e| ActorError::RoomOperationFailed(e.to_string()))
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
