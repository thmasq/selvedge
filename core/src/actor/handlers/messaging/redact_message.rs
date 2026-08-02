use crate::actor::MatrixActor;
use crate::actor::queue::QueueManager;
use matrix_sdk::ruma::OwnedTransactionId;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::room::RoomEvents;
use selvedge_shared::event::room::timeline_diff::TimelineDiffArgs;
use selvedge_shared::message::messaging::redact_message::RedactMessageArgs;
use selvedge_shared::model::TimelineDiff::ReplaceByEventId;
use selvedge_shared::model::{ActorError, DeliveryStatus, EncryptionStatus, TimelineContent};

pub async fn run(actor: &MatrixActor, args: RedactMessageArgs) -> Vec<ToShell> {
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

    let task = crate::actor::queue::OutboundTask {
        id: uuid::Uuid::new_v4().to_string(),
        room_id: args.room_id.clone(),
        payload: crate::actor::queue::TaskPayload::RedactMessage {
            event_id: args.event_id.clone(),
            reason: args.reason.clone(),
        },
    };

    let queue = actor.queue_manager.clone();
    let q_room_id = args.room_id.clone();

    wasm_bindgen_futures::spawn_local(async move {
        queue.lock().await.enqueue(task).await;
        QueueManager::poke(queue, &q_room_id);
    });

    let user_id = client_opt.as_ref().unwrap().user_id().unwrap().to_owned();

    let diff = ReplaceByEventId {
        event_id: args.event_id.clone(),
        entry: selvedge_shared::model::TimelineItem::Event(Box::new(
            selvedge_shared::model::EventItem {
                event_id: args.event_id.clone(),
                sender: user_id,
                sender_profile: None,
                timestamp: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(
                    (js_sys::Date::now() as u64)
                        .try_into()
                        .unwrap_or_else(|_| 0u32.into()),
                ),
                content: Box::new(TimelineContent::Redaction {
                    reason: args.reason,
                }),
                reactions: indexmap::IndexMap::new(),
                read_receipts: Vec::new(),
                delivery_status: DeliveryStatus::Sending {
                    txn_id: OwnedTransactionId::from(args.event_id.as_str()),
                    progress_pct: None,
                },
                in_reply_to: None,
                reply_details: None,
                is_edited: false,
                latest_edit: None,
                thread_root_id: None,
                is_highlight: false,
                should_group: false,
                encryption_status: EncryptionStatus::Unencrypted,
            },
        )),
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
