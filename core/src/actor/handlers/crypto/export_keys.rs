use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::crypto::export_keys::ExportKeysArgs;
use selvedge_shared::model::ActorError;

pub async fn run(_actor: &MatrixActor, args: ExportKeysArgs) -> Vec<ToShell> {
    vec![ToShell::Core(CoreEvents::CommandResult(CommandResultArgs {
            request_id: args.request_id,
            success: false,
            error: Some(ActorError::RoomOperationFailed("Manual file export is not supported in the web environment. Please use Account Recovery / Key Backup instead.".into())),
        }))]
}
