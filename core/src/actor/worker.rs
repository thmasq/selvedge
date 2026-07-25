use super::MatrixActor;
use futures::{StreamExt, channel::mpsc};
use gloo_worker::{HandlerId, Worker, WorkerScope};
use selvedge_shared::event::ToShell;
use selvedge_shared::message::ToActor;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

pub struct MatrixWorker {
    actor: Rc<MatrixActor>,
    bridge_id: Rc<RefCell<Option<HandlerId>>>,
}

impl Worker for MatrixWorker {
    type Input = ToActor;
    type Output = ToShell;
    type Message = ();

    fn create(scope: &WorkerScope<Self>) -> Self {
        let (tx, mut rx) = mpsc::unbounded();
        let actor = Rc::new(MatrixActor::new(tx));
        let bridge_id = Rc::new(RefCell::new(None));

        let scope_clone = scope.clone();
        let bridge_id_clone = bridge_id.clone();

        spawn_local(async move {
            while let Some(event) = rx.next().await {
                if let Some(id) = *bridge_id_clone.borrow() {
                    scope_clone.respond(id, event);
                }
            }
        });

        Self { actor, bridge_id }
    }

    fn update(&mut self, _scope: &WorkerScope<Self>, _msg: Self::Message) {}

    fn received(&mut self, scope: &WorkerScope<Self>, msg: Self::Input, id: HandlerId) {
        *self.bridge_id.borrow_mut() = Some(id);

        let actor = self.actor.clone();
        let scope = scope.clone();

        spawn_local(async move {
            let responses = actor.handle_message(msg).await;

            for response in responses {
                scope.respond(id, response);
            }
        });
    }
}
