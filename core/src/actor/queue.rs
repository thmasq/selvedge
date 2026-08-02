use futures::lock::Mutex;
use gloo_timers::callback::Timeout;
use matrix_sdk::ruma::{
    OwnedEventId, OwnedRoomId, OwnedTransactionId,
    events::{
        reaction::ReactionEventContent,
        relation::Annotation,
        room::message::{ReplacementMetadata, RoomMessageEventContent},
    },
};
use rexie::{ObjectStore, Rexie, TransactionMode};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

use crate::actor::queue::TaskPayload::RemoveReaction;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskPayload {
    SendMessage {
        txn_id: OwnedTransactionId,
        content: Box<RoomMessageEventContent>,
    },
    EditMessage {
        txn_id: OwnedTransactionId,
        target_event_id: OwnedEventId,
        new_content: Box<RoomMessageEventContent>,
    },
    RedactMessage {
        event_id: OwnedEventId,
        reason: Option<String>,
    },
    SendReaction {
        event_id: OwnedEventId,
        key: String,
    },
    RemoveReaction {
        target_event_id: OwnedEventId,
        reaction_event_id: Option<OwnedEventId>,
        key: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundTask {
    pub id: String,
    pub room_id: OwnedRoomId,
    pub payload: TaskPayload,
}

pub struct RoomQueueState {
    pub tasks: VecDeque<OutboundTask>,
    pub failures: u32,
    pub backoff_until: Option<f64>,
    pub is_sending: bool,
}

impl RoomQueueState {
    pub const fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
            failures: 0,
            backoff_until: None,
            is_sending: false,
        }
    }

    pub fn is_in_backoff(&self) -> bool {
        self.backoff_until
            .is_some_and(|until| js_sys::Date::now() < until)
    }
}

pub struct QueueManager {
    pub db: Option<Rexie>,
    pub queues: HashMap<OwnedRoomId, RoomQueueState>,
    pub client: Option<matrix_sdk::Client>,
}

impl QueueManager {
    pub fn new() -> Self {
        Self {
            db: None,
            queues: HashMap::new(),
            client: None,
        }
    }

    pub async fn init_persistence(manager: Rc<Mutex<Self>>) {
        let db_result = Rexie::builder("selvedge_queue_db")
            .version(1)
            .add_object_store(ObjectStore::new("outbound_tasks").key_path("id"))
            .build()
            .await;

        match db_result {
            Ok(db) => {
                let mut loaded_tasks = Vec::new();

                if let Ok(tx) = db.transaction(&["outbound_tasks"], TransactionMode::ReadOnly)
                    && let Ok(store) = tx.store("outbound_tasks")
                    && let Ok(all_tasks) = store.get_all(None, None).await
                {
                    for val in all_tasks {
                        if let Ok(task) = serde_wasm_bindgen::from_value::<OutboundTask>(val) {
                            loaded_tasks.push(task);
                        }
                    }
                }

                let mut this = manager.lock().await;
                for task in loaded_tasks {
                    let state = this
                        .queues
                        .entry(task.room_id.clone())
                        .or_insert_with(RoomQueueState::new);
                    state.tasks.push_back(task);
                }
                this.db = Some(db);
            }
            Err(e) => {
                web_sys::console::warn_1(&JsValue::from_str(&format!(
                    "Queue persistence disabled: {e:?}"
                )));
                manager.lock().await.db = None;
            }
        }
    }

    pub async fn enqueue(&mut self, mut task: OutboundTask) {
        if task.id.is_empty() {
            task.id = uuid::Uuid::new_v4().to_string();
        }

        let state = self
            .queues
            .entry(task.room_id.clone())
            .or_insert_with(RoomQueueState::new);

        if let RemoveReaction {
            target_event_id: rm_target,
            key: rm_key,
            ..
        } = &task.payload
        {
            if let Some(pos) = state.tasks.iter().position(|t| {
                if let TaskPayload::SendReaction { event_id, key } = &t.payload {
                    event_id == rm_target && key == rm_key
                } else {
                    false
                }
            }) {
                let removed_task = state.tasks.remove(pos).unwrap();
                self.remove_from_db(&removed_task.id).await;

                return;
            }
        }

        if let Some(db) = &self.db
            && let Ok(tx) = db.transaction(&["outbound_tasks"], TransactionMode::ReadWrite)
        {
            if let Ok(store) = tx.store("outbound_tasks")
                && let Ok(js_value) = serde_wasm_bindgen::to_value(&task)
            {
                let _ = store.add(&js_value, None).await;
            }
            let _ = tx.done().await;
        }

        state.tasks.push_back(task);
    }

    pub async fn remove_from_db(&self, id: &str) {
        if let Some(db) = &self.db
            && let Ok(tx) = db.transaction(&["outbound_tasks"], TransactionMode::ReadWrite)
        {
            if let Ok(store) = tx.store("outbound_tasks") {
                let key = JsValue::from_str(id);
                let _ = store.delete(key).await;
            }
            let _ = tx.done().await;
        }
    }

    pub fn poke(manager: Rc<Mutex<Self>>, room_id: &OwnedRoomId) {
        let r_id = room_id.clone();
        spawn_local(async move {
            let mut mgr = manager.lock().await;

            let client = match &mgr.client {
                Some(c) => c.clone(),
                None => return,
            };

            let Some(state) = mgr.queues.get_mut(&r_id) else {
                return;
            };

            if state.is_sending || state.is_in_backoff() || state.tasks.is_empty() {
                return;
            }

            state.is_sending = true;

            let task = state.tasks.front().unwrap().clone();
            let manager_clone = manager.clone();

            drop(mgr);

            let result = Self::execute_task(&client, &task).await;

            let mut mgr = manager_clone.lock().await;
            let state = mgr.queues.get_mut(&r_id).unwrap();

            if let Ok(returned_event_id) = result {
                if let TaskPayload::SendMessage { txn_id, .. } = &task.payload {
                    if let Some(real_event_id) = returned_event_id {
                        let fake_id = format!("~{txn_id}");
                        let mut needs_db_update = Vec::new();

                        for pending_task in &mut state.tasks {
                            let mut rewritten = false;
                            match &mut pending_task.payload {
                                TaskPayload::EditMessage {
                                    target_event_id, ..
                                } => {
                                    if target_event_id.as_str() == fake_id {
                                        *target_event_id = real_event_id.clone();
                                        rewritten = true;
                                    }
                                }
                                TaskPayload::SendReaction { event_id, .. }
                                | TaskPayload::RedactMessage { event_id, .. } => {
                                    if event_id.as_str() == fake_id {
                                        *event_id = real_event_id.clone();
                                        rewritten = true;
                                    }
                                }
                                _ => {}
                            }

                            if rewritten {
                                needs_db_update.push(pending_task.clone());
                            }
                        }

                        if !needs_db_update.is_empty() {
                            let mgr_for_db = manager_clone.clone();
                            spawn_local(async move {
                                let mut db_mgr = mgr_for_db.lock().await;
                                for updated_task in needs_db_update {
                                    db_mgr.enqueue(updated_task).await;
                                }
                            });
                        }
                    }
                }

                state.tasks.pop_front();
                state.failures = 0;
                state.is_sending = false;

                let mgr_for_db = manager_clone.clone();
                let task_id = task.id.clone();
                spawn_local(async move {
                    mgr_for_db.lock().await.remove_from_db(&task_id).await;
                });

                drop(mgr);

                Self::poke(manager_clone, &r_id);
            } else {
                state.failures += 1;
                state.is_sending = false;

                let mut delay_ms = 2000 * (2_u32.pow(state.failures - 1));
                if delay_ms > 60000 {
                    delay_ms = 60000;
                }

                state.backoff_until = Some(js_sys::Date::now() + f64::from(delay_ms));
                drop(mgr);

                let r_id_clone = r_id.clone();
                let manager_timeout = manager_clone.clone();

                Timeout::new(delay_ms, move || {
                    Self::poke(manager_timeout, &r_id_clone);
                })
                .forget();
            }
        });
    }

    async fn execute_task(
        client: &matrix_sdk::Client,
        task: &OutboundTask,
    ) -> Result<Option<OwnedEventId>, ()> {
        let Some(room) = client.get_room(&task.room_id) else {
            return Err(());
        };

        match &task.payload {
            TaskPayload::SendMessage { txn_id, content } => {
                let result = room
                    .send(*content.clone())
                    .with_transaction_id(txn_id.clone())
                    .await;
                result.map(|r| Some(r.response.event_id)).map_err(|_| ())
            }
            TaskPayload::EditMessage {
                txn_id,
                target_event_id,
                new_content,
            } => {
                let replacement = new_content
                    .clone()
                    .make_replacement(ReplacementMetadata::new(target_event_id.clone(), None));
                let result = room
                    .send(replacement)
                    .with_transaction_id(txn_id.clone())
                    .await;
                result.map(|r| Some(r.response.event_id)).map_err(|_| ())
            }
            TaskPayload::RedactMessage { event_id, reason } => {
                let redact_txn_id = OwnedTransactionId::from(task.id.clone());
                let result = room
                    .redact(event_id, reason.as_deref(), Some(redact_txn_id))
                    .await;
                result.map(|_| None).map_err(|_| ())
            }
            TaskPayload::SendReaction { event_id, key } => {
                let reaction =
                    ReactionEventContent::new(Annotation::new(event_id.clone(), key.clone()));
                let reaction_txn_id = OwnedTransactionId::from(task.id.clone());
                let result = room
                    .send(reaction)
                    .with_transaction_id(reaction_txn_id)
                    .await;
                result.map(|_| None).map_err(|_| ())
            }
            TaskPayload::RemoveReaction {
                reaction_event_id, ..
            } => {
                if let Some(event_id) = reaction_event_id {
                    let redact_txn_id = OwnedTransactionId::from(task.id.clone());
                    let result = room.redact(event_id, None, Some(redact_txn_id)).await;
                    result.map(|_| None).map_err(|_| ())
                } else {
                    Ok(None)
                }
            }

            #[allow(unreachable_patterns)]
            _ => Err(()),
        }
    }
}
