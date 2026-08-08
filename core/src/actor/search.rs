#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId};
use rexie::{ObjectStore, Rexie, TransactionMode};
use selvedge_shared::{EventItem, MessageContent, TimelineContent, TimelineItem};
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;
use std::rc::Rc;
use tantivy::Term;
use tantivy::directory::RamDirectory;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::Value;
use tantivy::schema::{FAST, Field, INDEXED, IndexRecordOption, STORED, STRING, Schema, TEXT};
use tantivy::{Directory, Index, IndexReader, IndexWriter, doc};
use wasm_bindgen::JsValue;

pub struct SearchEngine {
    pub inner: SearchIndex,
    pub db: Option<Rc<Rexie>>,
}

#[derive(Serialize, Deserialize)]
pub struct WalEntry {
    pub room_id: OwnedRoomId,
    pub event: EventItem,
}

#[allow(dead_code)]
pub struct SearchIndex {
    schema: Schema,

    // Schema Fields
    body_field: Field,
    event_id_field: Field,
    room_id_field: Field,
    timestamp_field: Field,
    sender_field: Field,

    // Hot Tier
    hot_dir: RamDirectory,
    hot_index: Index,
    hot_writer: IndexWriter,
    hot_reader: IndexReader,

    // Archive Tier
    archive_dir: RamDirectory,
    archive_index: Index,
    archive_writer: IndexWriter,
    archive_reader: IndexReader,

    // Quick lookup store
    event_store: HashMap<OwnedEventId, EventItem>,
}

impl SearchIndex {
    pub fn new() -> Self {
        let mut schema_builder = Schema::builder();
        let body_field = schema_builder.add_text_field("body", TEXT | STORED);
        let event_id_field = schema_builder.add_text_field("event_id", STRING | STORED);
        let room_id_field = schema_builder.add_text_field("room_id", STRING | STORED);
        let timestamp_field = schema_builder.add_u64_field("timestamp", INDEXED | STORED | FAST);
        let sender_field = schema_builder.add_text_field("sender", STRING | STORED);
        let schema = schema_builder.build();

        let hot_dir = RamDirectory::create();
        let hot_index = Index::open_or_create(hot_dir.clone(), schema.clone()).unwrap();

        let hot_writer = hot_index.writer(15_000_000).unwrap();
        let hot_reader = hot_index.reader().unwrap();

        let archive_dir = RamDirectory::create();
        let archive_index = Index::open_or_create(archive_dir.clone(), schema.clone()).unwrap();

        let archive_writer = archive_index.writer(15_000_000).unwrap();
        let archive_reader = archive_index.reader().unwrap();

        Self {
            schema,
            body_field,
            event_id_field,
            room_id_field,
            timestamp_field,
            sender_field,
            hot_dir,
            hot_index,
            hot_writer,
            hot_reader,
            archive_dir,
            archive_index,
            archive_writer,
            archive_reader,
            event_store: HashMap::new(),
        }
    }

    /// Indexes a new incoming message into the HOT index.
    pub(crate) fn index_item(&mut self, room_id: &OwnedRoomId, item: &TimelineItem) {
        if let TimelineItem::Event(event_item) = item
            && let TimelineContent::Message(MessageContent::Text { body, .. }) =
                &*event_item.content
        {
            self.event_store
                .insert(event_item.event_id.clone(), *event_item.clone());

            self.hot_writer
                .add_document(doc!(
                    self.body_field => body.clone(),
                    self.event_id_field => event_item.event_id.to_string(),
                    self.room_id_field => room_id.to_string(),
                    self.timestamp_field => u64::from(event_item.timestamp.0),
                    self.sender_field => event_item.sender.to_string()
                ))
                .unwrap();

            let _ = self.hot_writer.commit();
        }
    }

    /// Searches both the Hot and Archive indexes and deduplicates results.
    pub(crate) fn search(
        &self,
        room_id_filter: Option<&OwnedRoomId>,
        query_str: &str,
        limit: usize,
    ) -> Vec<EventItem> {
        if query_str.trim().is_empty() {
            return vec![];
        }

        let query_parser = QueryParser::for_index(&self.hot_index, vec![self.body_field]);
        let Ok(user_query) = query_parser.parse_query(query_str) else {
            return vec![];
        };

        let query: Box<dyn tantivy::query::Query> = if let Some(r_id) = room_id_filter {
            let room_id_term = Term::from_field_text(self.room_id_field, r_id.as_str());
            let room_id_query = Box::new(TermQuery::new(room_id_term, IndexRecordOption::Basic));

            Box::new(BooleanQuery::new(vec![
                (Occur::Must, user_query),
                (Occur::Must, room_id_query),
            ]))
        } else {
            user_query
        };

        let mut matched_events = HashMap::new();

        let mut search_tier = |reader: &IndexReader| {
            let searcher = reader.searcher();
            let top_docs_collector =
                tantivy::collector::TopDocs::with_limit(limit * 2).order_by_score();

            if let Ok(top_docs) = searcher.search(&*query, &top_docs_collector) {
                for (_score, doc_address) in top_docs {
                    if let Ok(retrieved_doc) = searcher.doc::<tantivy::TantivyDocument>(doc_address)
                        && let (Some(body), Some(event_id_str), Some(sender_str), Some(timestamp)) = (
                            retrieved_doc
                                .get_first(self.body_field)
                                .and_then(|v| v.as_str()),
                            retrieved_doc
                                .get_first(self.event_id_field)
                                .and_then(|v| v.as_str()),
                            retrieved_doc
                                .get_first(self.sender_field)
                                .and_then(|v| v.as_str()),
                            retrieved_doc
                                .get_first(self.timestamp_field)
                                .and_then(|v| v.as_u64()),
                        )
                        && let (Ok(event_id), Ok(sender)) = (
                            matrix_sdk::ruma::OwnedEventId::try_from(event_id_str),
                            matrix_sdk::ruma::OwnedUserId::try_from(sender_str),
                        )
                    {
                        let content = Box::new(selvedge_shared::model::TimelineContent::Message(
                            selvedge_shared::model::MessageContent::Text {
                                body: body.to_string(),
                                formatted: None,
                                previews: vec![],
                            },
                        ));

                        let event_item = selvedge_shared::model::EventItem {
                            event_id: event_id.clone(),
                            sender,
                            sender_profile: None,
                            timestamp: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(
                                timestamp.try_into().unwrap_or_else(|_| 0u32.into()),
                            ),
                            content,
                            reactions: indexmap::IndexMap::new(),
                            read_receipts: Vec::new(),
                            delivery_status: selvedge_shared::model::DeliveryStatus::Synced,
                            in_reply_to: None,
                            reply_details: None,
                            is_edited: false,
                            latest_edit: None,
                            thread_root_id: None,
                            is_own_mention: false,
                            is_highlight: false,
                            should_group: false,
                            encryption_status:
                                selvedge_shared::model::EncryptionStatus::Unencrypted,
                        };

                        matched_events.insert(event_id, event_item);
                    }
                }
            }
        };

        search_tier(&self.hot_reader);
        search_tier(&self.archive_reader);

        let mut final_events: Vec<EventItem> = matched_events.into_values().collect();
        final_events.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        final_events.into_iter().take(limit).collect()
    }

    /// Serializes the entire Archive `RamDirectory` into a flat map of (Filename -> Bytes).
    #[allow(clippy::unused_self)]
    pub fn export_archive_to_bytes(&self) -> HashMap<String, Vec<u8>> {
        // TODO: Implement a custom TrackingDirectory wrapper since RamDirectory lacks list()
        HashMap::new()
    }

    /// Loads the binary blobs from `IndexedDB` into a fresh Archive `RamDirectory`
    pub fn load_archive_from_bytes(&mut self, files: HashMap<String, Vec<u8>>) {
        let new_dir = RamDirectory::create();
        for (name, data) in files {
            if let Ok(mut write_handle) = new_dir.open_write(Path::new(&name)) {
                let _ = write_handle.write_all(&data);
                let _ = write_handle.flush();
            }
        }

        if let Ok(index) = Index::open(new_dir.clone()) {
            self.archive_dir = new_dir;
            self.archive_index = index;
            self.archive_writer = self.archive_index.writer(15_000_000).unwrap();
            self.archive_reader = self.archive_index.reader().unwrap();
        }
    }

    /// Keeps the Hot Index from crashing the browser if persistent storage is denied.
    pub(crate) fn prune_hot_index(&mut self, max_docs: u64) {
        let searcher = self.hot_reader.searcher();
        if searcher.num_docs() > max_docs {
            let threshold = (js_sys::Date::now() as u64) - (7 * 24 * 60 * 60 * 1000);

            let query = tantivy::query::RangeQuery::new(
                std::ops::Bound::Included(tantivy::Term::from_field_u64(self.timestamp_field, 0)),
                std::ops::Bound::Excluded(tantivy::Term::from_field_u64(
                    self.timestamp_field,
                    threshold,
                )),
            );

            let _ = self.hot_writer.delete_query(Box::new(query));
            let _ = self.hot_writer.commit();
        }
    }

    /// Forces the Archive to stay under bounds with a 10% headroom to prevent thrashing
    pub(crate) fn prune_archive_index(&mut self, max_docs: usize) {
        let searcher = self.archive_reader.searcher();
        let current_docs = searcher.num_docs() as usize;

        if current_docs > max_docs {
            let target_docs = max_docs - (max_docs / 10);
            let excess_count = current_docs.saturating_sub(target_docs);

            let mut min_ts = 0u64;
            let mut max_ts = js_sys::Date::now() as u64;
            let mut threshold = 0u64;

            for _ in 0..15 {
                let mid = min_ts + (max_ts - min_ts) / 2;
                let query = tantivy::query::RangeQuery::new(
                    std::ops::Bound::Included(tantivy::Term::from_field_u64(
                        self.timestamp_field,
                        0,
                    )),
                    std::ops::Bound::Excluded(tantivy::Term::from_field_u64(
                        self.timestamp_field,
                        mid,
                    )),
                );

                if let Ok(count) = searcher.search(&query, &tantivy::collector::Count) {
                    if count > excess_count {
                        max_ts = mid;
                    } else {
                        min_ts = mid;
                        threshold = mid;
                    }
                }
            }

            let delete_query = tantivy::query::RangeQuery::new(
                std::ops::Bound::Included(tantivy::Term::from_field_u64(self.timestamp_field, 0)),
                std::ops::Bound::Excluded(tantivy::Term::from_field_u64(
                    self.timestamp_field,
                    threshold,
                )),
            );
            let _ = self.archive_writer.delete_query(Box::new(delete_query));
            let _ = self.archive_writer.commit();
        }
    }

    /// Physically removes a document from both tiers
    pub(crate) fn delete_item(&mut self, event_id: &matrix_sdk::ruma::OwnedEventId) {
        let term = tantivy::Term::from_field_text(self.event_id_field, event_id.as_str());
        let query = tantivy::query::TermQuery::new(term, tantivy::schema::IndexRecordOption::Basic);

        let _ = self.hot_writer.delete_query(Box::new(query.clone()));
        let _ = self.hot_writer.commit();

        let _ = self.archive_writer.delete_query(Box::new(query));
        let _ = self.archive_writer.commit();
    }

    /// Deletes the old version (if any) and indexes the new one
    pub(crate) fn upsert_item(&mut self, room_id: &OwnedRoomId, item: &TimelineItem) {
        if let TimelineItem::Event(event_item) = item {
            self.delete_item(&event_item.event_id);

            if let TimelineContent::Message(MessageContent::Text { body, .. }) =
                &*event_item.content
            {
                self.hot_writer
                    .add_document(doc!(
                        self.body_field => body.clone(),
                        self.event_id_field => event_item.event_id.to_string(),
                        self.room_id_field => room_id.to_string(),
                        self.timestamp_field => u64::from(event_item.timestamp.0),
                        self.sender_field => event_item.sender.to_string()
                    ))
                    .unwrap();
                let _ = self.hot_writer.commit();
            }
        }
    }
}

impl SearchEngine {
    /// Creates the engine purely in RAM (synchronous)
    pub fn new() -> Self {
        Self {
            inner: SearchIndex::new(),
            db: None,
        }
    }

    /// Attempts to initialize the Rexie database. If the user refuses storage,
    /// it degrades gracefully into an ephemeral memory-only search.
    pub async fn init_persistence(engine: Rc<futures::lock::Mutex<Self>>) {
        let db_result = Rexie::builder("selvedge_search_db")
            .version(1)
            .add_object_store(ObjectStore::new("tantivy_archive"))
            .add_object_store(ObjectStore::new("uncommitted_events").auto_increment(true))
            .build()
            .await;

        match db_result {
            Ok(db) => {
                let recovered = Self::recover_archive_on_startup(&db)
                    .await
                    .unwrap_or_default();
                let mut this = engine.lock().await;
                this.inner.load_archive_from_bytes(recovered);

                // Store inside Rc::new
                this.db = Some(Rc::new(db));
            }
            Err(e) => {
                web_sys::console::warn_1(&JsValue::from_str(&format!(
                    "Search persistence disabled: {e:?}"
                )));
                engine.lock().await.db = None;
            }
        }
    }

    /// Upserts an item in RAM, and safely appends it to the WAL in the background.
    pub async fn upsert_live_event(&mut self, room_id: &OwnedRoomId, item: &TimelineItem) {
        self.inner.upsert_item(room_id, item);

        if let Some(db) = &self.db
            && let TimelineItem::Event(event_item) = item
        {
            let entry = WalEntry {
                room_id: room_id.clone(),
                event: *event_item.clone(),
            };

            if let Ok(tx) = db.transaction(&["uncommitted_events"], TransactionMode::ReadWrite) {
                if let Ok(store) = tx.store("uncommitted_events")
                    && let Ok(js_value) = serde_wasm_bindgen::to_value(&entry)
                {
                    let _ = store.add(&js_value, None).await;
                }
                let _ = tx.done().await;
            }
        }
    }

    /// Run this periodically (e.g., every 10 mins) to move WAL events into the Archive,
    /// prune old data, and save the blobs via an Atomic Transaction.
    pub async fn run_archiving_cycle(engine: Rc<futures::lock::Mutex<Self>>) {
        let (db_opt, segment_ids_to_merge) = {
            let mut this = engine.lock().await;
            let segment_ids = this
                .inner
                .hot_index
                .searchable_segment_ids()
                .unwrap_or_default();

            if this.db.is_none() {
                this.inner.prune_hot_index(10_000);
            }
            (this.db.clone(), segment_ids)
        };

        if let Some(db) = db_opt {
            let mut uncommitted = Vec::new();
            if let Ok(tx) = db.transaction(&["uncommitted_events"], TransactionMode::ReadOnly)
                && let Ok(store) = tx.store("uncommitted_events")
                && let Ok(all) = store.get_all(None, None).await
            {
                for val in all {
                    if let Ok(entry) = serde_wasm_bindgen::from_value::<WalEntry>(val) {
                        uncommitted.push(entry);
                    }
                }
            }

            if !uncommitted.is_empty() {
                let exported_files = {
                    let mut this = engine.lock().await;
                    for entry in uncommitted {
                        if let TimelineContent::Message(MessageContent::Text { body, .. }) =
                            &*entry.event.content
                        {
                            let _ = this.inner.archive_writer.add_document(doc!(
                                    this.inner.body_field => body.clone(),
                                    this.inner.event_id_field => entry.event.event_id.to_string(),
                                    this.inner.room_id_field => entry.room_id.to_string(),
                                    this.inner.timestamp_field => u64::from(entry.event.timestamp.0),
                                    this.inner.sender_field => entry.event.sender.to_string()
    				));
                        }
                    }
                    let _ = this.inner.archive_writer.commit();
                    this.inner.prune_archive_index(150_000);
                    this.inner.export_archive_to_bytes()
                };

                let mut this = engine.lock().await;
                if let Ok(segment_ids) = this.inner.archive_index.searchable_segment_ids()
                    && !segment_ids.is_empty()
                {
                    let _ = this.inner.archive_writer.merge(&segment_ids).await;
                }
                let _ = this.inner.archive_writer.garbage_collect_files().await;
                drop(this);

                if let Ok(tx) = db.transaction(
                    &["tantivy_archive", "uncommitted_events"],
                    TransactionMode::ReadWrite,
                ) {
                    if let (Ok(archive_store), Ok(wal_store)) =
                        (tx.store("tantivy_archive"), tx.store("uncommitted_events"))
                    {
                        let _ = archive_store.clear().await;
                        for (filename, bytes) in exported_files {
                            let key = JsValue::from_str(&filename);
                            let val = js_sys::Uint8Array::from(&bytes[..]);
                            let _ = archive_store.add(&val, Some(&key)).await;
                        }
                        let _ = wal_store.clear().await;
                    }
                    let _ = tx.done().await;
                }
            }
        } else {
            let mut this = engine.lock().await;
            if !segment_ids_to_merge.is_empty() {
                let _ = this.inner.hot_writer.merge(&segment_ids_to_merge).await;
            }
            let _ = this.inner.hot_writer.garbage_collect_files().await;
            drop(this);
        }
    }

    /// Internal Startup Recovery
    async fn recover_archive_on_startup(
        db: &Rexie,
    ) -> Result<HashMap<String, Vec<u8>>, rexie::Error> {
        let tx = db.transaction(&["tantivy_archive"], TransactionMode::ReadOnly)?;
        let store = tx.store("tantivy_archive")?;

        let mut recovered_files = HashMap::new();
        let keys = store.get_all_keys(None, None).await?;
        let all_blobs = store.get_all(None, None).await?;

        for (key, val) in keys.into_iter().zip(all_blobs) {
            if let Some(filename) = key.as_string() {
                let array = js_sys::Uint8Array::new(&val);
                recovered_files.insert(filename, array.to_vec());
            }
        }
        Ok(recovered_files)
    }
}
