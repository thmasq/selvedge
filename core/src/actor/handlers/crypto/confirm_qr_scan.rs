use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::confirm_qr_scan::ConfirmQrScanArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: ConfirmQrScanArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        if let Some(request) = client
            .encryption()
            .get_verification_request(&args.user_id, &args.flow_id)
            .await
        {
            match matrix_sdk::encryption::verification::QrVerificationData::from_bytes(
                &args.scanned_data,
            ) {
                Ok(qr_data) => match request.scan_qr_code(qr_data).await {
                    Ok(Some(qr)) => match qr.confirm().await {
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
                    },
                    Ok(None) => vec![ToShell::Core(CoreEvents::CommandResult(
                        CommandResultArgs {
                            request_id: args.request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(
                                "Scanned QR code does not match this verification flow".into(),
                            )),
                        },
                    ))],
                    Err(e) => vec![ToShell::Core(CoreEvents::CommandResult(
                        CommandResultArgs {
                            request_id: args.request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(e.to_string())),
                        },
                    ))],
                },
                Err(e) => vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(format!(
                            "Invalid QR data: {e}"
                        ))),
                    },
                ))],
            }
        } else {
            vec![ToShell::Core(CoreEvents::CommandResult(
                CommandResultArgs {
                    request_id: args.request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(
                        "Verification flow not found".into(),
                    )),
                },
            ))]
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
