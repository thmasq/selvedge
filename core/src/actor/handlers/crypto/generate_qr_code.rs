use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::event::crypto::CryptoEvents;
use selvedge_shared::event::crypto::qr_code_generated::QrCodeGeneratedArgs;
use selvedge_shared::message::crypto::generate_qr_code::GenerateQrCodeArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: GenerateQrCodeArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        if let Some(request) = client
            .encryption()
            .get_verification_request(&args.user_id, &args.flow_id)
            .await
        {
            match request.generate_qr_code().await {
                Ok(Some(qr)) => {
                    let bytes = qr.to_bytes().unwrap_or_default();
                    actor
                        .active_qr_verifications
                        .borrow_mut()
                        .insert(args.flow_id.clone(), qr);
                    vec![ToShell::Crypto(CryptoEvents::QrCodeGenerated(
                        QrCodeGeneratedArgs {
                            request_id: args.request_id,
                            payload: bytes,
                        },
                    ))]
                }
                Ok(None) => vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(
                            "Could not generate QR code (not supported or invalid state)".into(),
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
