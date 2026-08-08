use crate::actor::MatrixActor;
use crate::actor::queue::{OutboundTask, QueueManager, TaskPayload};
use js_sys::Date;
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use matrix_sdk::ruma::{MilliSecondsSinceUnixEpoch, TransactionId};

use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::room::RoomEvents;
use selvedge_shared::event::room::timeline_diff::TimelineDiffArgs;
use selvedge_shared::message::messaging::edit_message::EditMessageArgs;
use selvedge_shared::model::{
    ActorError, DeliveryStatus, EncryptionStatus, EventItem, TimelineContent, TimelineDiff,
    TimelineItem,
};

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

    let trimmed_html = selvedge_shared::message::markdown::parse_matrix_markdown(&args.new_body);

    let mut ruma_content = RoomMessageEventContent::text_html(args.new_body.clone(), trimmed_html);

    if let Some(mentions) = selvedge_shared::message::markdown::extract_mentions(&args.new_body) {
        ruma_content.mentions = Some(mentions);
    }

    let local_echo_content = TimelineContent::from(ruma_content.clone());

    let txn_id = TransactionId::new();

    let task_payload = TaskPayload::EditMessage {
        txn_id: txn_id.clone(),
        target_event_id: args.event_id.clone(),
        new_content: Box::new(ruma_content),
    };

    let task = OutboundTask {
        id: uuid::Uuid::new_v4().to_string(),
        room_id: args.room_id.clone(),
        payload: task_payload,
    };

    let queue = actor.queue_manager.clone();
    let q_room_id = args.room_id.clone();

    wasm_bindgen_futures::spawn_local(async move {
        queue.lock().await.enqueue(task).await;
        QueueManager::poke(queue, &q_room_id);
    });

    let user_id = client_opt.as_ref().unwrap().user_id().unwrap().to_owned();

    let diff = TimelineDiff::ReplaceByEventId {
        event_id: args.event_id.clone(),
        entry: TimelineItem::Event(Box::new(EventItem {
            event_id: args.event_id.clone(),
            sender: user_id,
            sender_profile: None,
            timestamp: MilliSecondsSinceUnixEpoch(
                (Date::now() as u64)
                    .try_into()
                    .unwrap_or_else(|_| 0u32.into()),
            ),
            content: Box::new(local_echo_content.clone()),
            reactions: indexmap::IndexMap::new(),
            read_receipts: Vec::new(),
            delivery_status: DeliveryStatus::Sending {
                txn_id: txn_id.clone(),
                progress_pct: None,
            },
            in_reply_to: None,
            reply_details: None,
            is_edited: true,
            latest_edit: Some(Box::new(local_echo_content)),
            thread_root_id: None,
            is_own_mention: false,
            is_highlight: false,
            is_trusted: true,
            should_group: false,
            encryption_status: EncryptionStatus::Unencrypted,
        })),
    };

    let local_echo_event = ToShell::Room(RoomEvents::TimelineDiff(TimelineDiffArgs {
        room_id: args.room_id.clone(),
        diff: vec![diff],
    }));

    vec![
        local_echo_event,
        ToShell::Core(CoreEvents::CommandResult(CommandResultArgs {
            request_id: args.request_id,
            success: true,
            error: None,
        })),
    ]
}
