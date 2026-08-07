use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::background_error::BackgroundErrorArgs;
use selvedge_shared::event::room::RoomEvents;
use selvedge_shared::event::room::history_loaded::HistoryLoadedArgs;
use selvedge_shared::message::room::load_history::LoadHistoryArgs;
use selvedge_shared::model::ActorError;

pub async fn run(actor: &MatrixActor, args: LoadHistoryArgs) -> Vec<ToShell> {
    let timeline = actor.active_timelines.borrow().get(&args.room_id).cloned();

    if let Some(timeline) = timeline {
        match timeline.paginate_backwards(20).await {
            Ok(start_of_room) => {
                return vec![ToShell::Room(RoomEvents::HistoryLoaded(
                    HistoryLoadedArgs {
                        room_id: args.room_id,
                        start_of_room,
                    },
                ))];
            }
            Err(e) => {
                return vec![ToShell::Core(CoreEvents::BackgroundError(
                    BackgroundErrorArgs {
                        error: ActorError::PaginationFailed(e.to_string()),
                    },
                ))];
            }
        }
    }
    vec![]
}
