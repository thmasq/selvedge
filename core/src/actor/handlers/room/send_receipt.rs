use crate::actor::MatrixActor;
use matrix_sdk::ruma::api::client::receipt::create_receipt::v3::ReceiptType;
use matrix_sdk::ruma::events::receipt::ReceiptThread;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::room::send_receipt::{ReceiptTypeWrapper, SendReceiptArgs};
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: SendReceiptArgs) -> Vec<ToShell> {
    let room = actor
        .client
        .borrow()
        .as_ref()
        .and_then(|c| c.get_room(&args.room_id));

    if let Some(room) = room {
        let result = match args.receipt_type {
            ReceiptTypeWrapper::Read => {
                room.send_single_receipt(
                    ReceiptType::Read,
                    ReceiptThread::Unthreaded,
                    args.event_id,
                )
                .await
            }
            ReceiptTypeWrapper::ReadPrivate => {
                room.send_single_receipt(
                    ReceiptType::ReadPrivate,
                    ReceiptThread::Unthreaded,
                    args.event_id,
                )
                .await
            }
            ReceiptTypeWrapper::FullyRead => {
                let mut receipts = matrix_sdk::room::Receipts::new();
                receipts.fully_read = Some(args.event_id);
                room.send_multiple_receipts(receipts).await
            }
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
