use gloo_timers::callback::Timeout;
use matrix_sdk::ruma::{
    OwnedEventId, OwnedRoomId, OwnedTransactionId,
    events::{
        reaction::ReactionEventContent, relation::Annotation,
        room::message::RoomMessageEventContent,
    },
};
use rexie::{ObjectStore, Rexie, TransactionMode};
use selvedge_shared::model::MessageContent;
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    rc::Rc,
};
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::spawn_local;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskPayload {
    SendMessage {
        txn_id: OwnedTransactionId,
        content: RoomMessageEventContent,
    },
    SendReaction {
        event_id: OwnedEventId,
        key: String,
    },
    EditMessage {
        event_id: OwnedEventId,
        new_content: MessageContent,
    },
    RedactMessage {
        event_id: OwnedEventId,
        reason: Option<String>,
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
    pub fn new() -> Self {
        Self {
            tasks: VecDeque::new(),
            failures: 0,
            backoff_until: None,
            is_sending: false,
        }
    }

    pub fn is_in_backoff(&self) -> bool {
        if let Some(until) = self.backoff_until {
            js_sys::Date::now() < until
        } else {
            false
        }
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

    pub async fn init_persistence(&mut self) {
        let db_result = Rexie::builder("selvedge_queue_db")
            .version(1)
            .add_object_store(ObjectStore::new("outbound_tasks").key_path("id"))
            .build()
            .await;

        match db_result {
            Ok(db) => {
                if let Ok(tx) = db.transaction(&["outbound_tasks"], TransactionMode::ReadOnly) {
                    if let Ok(store) = tx.store("outbound_tasks") {
                        if let Ok(all_tasks) = store.get_all(None, None).await {
                            for val in all_tasks {
                                if let Ok(task) =
                                    serde_wasm_bindgen::from_value::<OutboundTask>(val)
                                {
                                    let state = self
                                        .queues
                                        .entry(task.room_id.clone())
                                        .or_insert_with(RoomQueueState::new);
                                    state.tasks.push_back(task);
                                }
                            }
                        }
                    }
                }
                self.db = Some(db);
            }
            Err(e) => {
                web_sys::console::warn_1(&JsValue::from_str(&format!(
                    "Queue persistence disabled: {:?}",
                    e
                )));
                self.db = None;
            }
        }
    }

    pub async fn enqueue(&mut self, mut task: OutboundTask) {
        if task.id.is_empty() {
            task.id = uuid::Uuid::new_v4().to_string();
        }

        if let Some(db) = &self.db {
            if let Ok(tx) = db.transaction(&["outbound_tasks"], TransactionMode::ReadWrite) {
                if let Ok(store) = tx.store("outbound_tasks") {
                    if let Ok(js_value) = serde_wasm_bindgen::to_value(&task) {
                        let _ = store.add(&js_value, None).await;
                    }
                }
                let _ = tx.done().await;
            }
        }

        let state = self
            .queues
            .entry(task.room_id.clone())
            .or_insert_with(RoomQueueState::new);
        state.tasks.push_back(task);
    }

    pub async fn remove_from_db(&self, id: &str) {
        if let Some(db) = &self.db {
            if let Ok(tx) = db.transaction(&["outbound_tasks"], TransactionMode::ReadWrite) {
                if let Ok(store) = tx.store("outbound_tasks") {
                    let key = JsValue::from_str(id);
                    let _ = store.delete(key).await;
                }
                let _ = tx.done().await;
            }
        }
    }

    pub fn poke(manager: Rc<RefCell<QueueManager>>, room_id: &OwnedRoomId) {
        let mut mgr = manager.borrow_mut();

        let client = match &mgr.client {
            Some(c) => c.clone(),
            None => return,
        };

        let state = match mgr.queues.get_mut(room_id) {
            Some(s) => s,
            None => return,
        };

        if state.is_sending || state.is_in_backoff() || state.tasks.is_empty() {
            return;
        }

        state.is_sending = true;

        let task = state.tasks.front().unwrap().clone();
        let r_id = room_id.clone();
        let manager_clone = manager.clone();

        spawn_local(async move {
            let success = Self::execute_task(&client, &task).await;

            let mut mgr = manager_clone.borrow_mut();
            if success {
                let state = mgr.queues.get_mut(&r_id).unwrap();
                state.tasks.pop_front();
                state.failures = 0;
                state.is_sending = false;

                let mgr_for_db = manager_clone.clone();
                let task_id = task.id.clone();
                spawn_local(async move {
                    mgr_for_db.borrow().remove_from_db(&task_id).await;
                });

                drop(mgr);

                QueueManager::poke(manager_clone, &r_id);
            } else {
                let state = mgr.queues.get_mut(&r_id).unwrap();
                state.failures += 1;
                state.is_sending = false;

                let mut delay_ms = 2000 * (2_u32.pow(state.failures - 1));
                if delay_ms > 60000 {
                    delay_ms = 60000;
                }

                state.backoff_until = Some(js_sys::Date::now() + (delay_ms as f64));
                drop(mgr);

                let r_id_clone = r_id.clone();
                let manager_timeout = manager_clone.clone();

                Timeout::new(delay_ms, move || {
                    QueueManager::poke(manager_timeout, &r_id_clone);
                })
                .forget();
            }
        });
    }

    async fn execute_task(client: &matrix_sdk::Client, task: &OutboundTask) -> bool {
        let room = match client.get_room(&task.room_id) {
            Some(r) => r,
            None => return false, // Room doesn't exist locally yet
        };

        match &task.payload {
            TaskPayload::SendMessage { txn_id, content } => {
                let result = room
                    .send(content.clone())
                    .with_transaction_id(txn_id.clone())
                    .await;
                result.is_ok()
            }

            TaskPayload::SendReaction { event_id, key } => {
                let reaction =
                    ReactionEventContent::new(Annotation::new(event_id.clone(), key.clone()));

                let reaction_txn_id = OwnedTransactionId::from(task.id.clone());
                let result = room
                    .send(reaction)
                    .with_transaction_id(reaction_txn_id)
                    .await;
                result.is_ok()
            }

            // ... handle other variants
            _ => false,
        }
    }
}
