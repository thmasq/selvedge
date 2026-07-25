use crate::actor::MatrixActor;
use selvedge_shared::event::ToShell;
use selvedge_shared::message::room::close_room::CloseRoomArgs;

pub async fn run(actor: &MatrixActor, args: CloseRoomArgs) -> Vec<ToShell> {
    actor.active_timelines.borrow_mut().remove(&args.room_id);
    vec![]
}
