use crate::actor::MatrixActor;
use matrix_sdk::ruma::events::room::message::{
    AddMentions, ForwardThread, RoomMessageEventContent,
};
use matrix_sdk::ruma::events::{AnyMessageLikeEvent, AnyTimelineEvent, MessageLikeEvent};
use pulldown_cmark::{Options, Parser, html};
use std::str::FromStr;

use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::messaging::send_message::SendMessageArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: SendMessageArgs) -> Vec<ToShell> {
    let client_opt = actor.client.borrow().clone();

    let Some((client, room)) = client_opt
        .as_ref()
        .and_then(|c| c.get_room(&args.room_id).map(|r| (c.clone(), r)))
    else {
        return vec![ToShell::Core(CoreEvents::CommandResult(
            CommandResultArgs {
                request_id: args.request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            },
        ))];
    };

    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(&args.body, options);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);

    let safe_html = selvedge_shared::sanitize_matrix_html(&raw_html);
    let trimmed_html = safe_html.trim().to_string();

    let mut ruma_content =
        RoomMessageEventContent::text_html(args.body.clone(), trimmed_html.clone());

    if let Some(event_id) = &args.reply_to
        && let Ok(event) = room.event(event_id, None).await
        && let Ok(any_event) = event.kind.into_raw().deserialize()
    {
        let full_event = any_event.into_full_event(args.room_id.clone());
        if let AnyTimelineEvent::MessageLike(AnyMessageLikeEvent::RoomMessage(
            MessageLikeEvent::Original(orig_msg),
        )) = full_event
        {
            ruma_content =
                ruma_content.make_reply_to(&orig_msg, ForwardThread::Yes, AddMentions::Yes);
        }
    }

    let txn_id = matrix_sdk::ruma::TransactionId::new();

    let task_payload = crate::actor::queue::TaskPayload::SendMessage {
        txn_id: txn_id.clone(),
        content: Box::new(ruma_content.clone()),
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

    let user_id = client.user_id().unwrap().to_owned();

    let fake_event_id = matrix_sdk::ruma::OwnedEventId::from_str(&format!("~{txn_id}")).unwrap();

    let local_echo = selvedge_shared::model::EventItem {
        event_id: fake_event_id,
        sender: user_id,
        sender_profile: None,
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        timestamp: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(
            (js_sys::Date::now() as u64)
                .try_into()
                .unwrap_or_else(|_| 0u32.into()),
        ),
        content: Box::new(selvedge_shared::model::TimelineContent::from(ruma_content)),
        reactions: indexmap::IndexMap::new(),
        read_receipts: Vec::new(),
        delivery_status: selvedge_shared::model::DeliveryStatus::Sending(txn_id),
        in_reply_to: args.reply_to,
        reply_details: None,
        is_edited: false,
        latest_edit: None,
        thread_root_id: None,
        is_highlight: false,
        should_group: false,
        encryption_status: selvedge_shared::model::EncryptionStatus::Unencrypted,
    };

    let diff = selvedge_shared::model::TimelineDiff::PushBack {
        entry: selvedge_shared::model::TimelineItem::Event(Box::new(local_echo)),
    };

    let local_echo_event = ToShell::Room(selvedge_shared::event::room::RoomEvents::TimelineDiff(
        selvedge_shared::event::room::timeline_diff::TimelineDiffArgs {
            room_id: args.room_id.clone(),
            diff: vec![diff],
        },
    ));

    vec![
        local_echo_event,
        ToShell::Core(CoreEvents::CommandResult(CommandResultArgs {
            request_id: args.request_id,
            success: true,
            error: None,
        })),
    ]
}
