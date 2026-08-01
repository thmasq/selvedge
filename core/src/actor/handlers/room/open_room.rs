use crate::actor::MatrixActor;
use crate::actor::mapping::{map_timeline_diff, map_timeline_item_safe};
use futures::StreamExt;
use matrix_sdk_ui::timeline::RoomExt;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::room::RoomEvents;
use selvedge_shared::event::room::room_details_update::RoomDetailsUpdateArgs;
use selvedge_shared::event::room::timeline_diff::TimelineDiffArgs;
use selvedge_shared::message::room::open_room::OpenRoomArgs;
use selvedge_shared::model::{RoomDetails, RoomPermissions, RoomTrustLevel, TimelineDiff};
use std::collections::{HashMap, HashSet, VecDeque};
use wasm_bindgen_futures::spawn_local;

pub async fn run(actor: &MatrixActor, args: OpenRoomArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client
        && let Some(room) = client.get_room(&args.room_id)
    {
        let has_timeline = actor.active_timelines.borrow().contains_key(&args.room_id);

        if !has_timeline && let Ok(timeline) = room.timeline_builder().build().await {
            let is_encrypted = room.encryption_state().is_encrypted();

            let trust_level = if is_encrypted {
                RoomTrustLevel::Trusted
            } else {
                RoomTrustLevel::Plain
            };

            actor.send_event(ToShell::Room(RoomEvents::RoomDetailsUpdate(
                RoomDetailsUpdateArgs {
                    room_id: args.room_id.clone(),
                    details: RoomDetails {
                        room_id: args.room_id.clone(),
                        name: room.name(),
                        topic: room.topic(),
                        avatar_url: room.avatar_url(),
                        members: HashMap::new(),
                        timeline: VecDeque::new(),
                        typing_users: HashSet::new(),
                        active_call: None,
                        is_encrypted,
                        trust_level,
                        permissions: RoomPermissions::default(),
                        prev_batch: None,
                        next_batch: None,
                        fully_read_marker: None,
                    },
                },
            )));

            let (items, mut stream) = timeline.subscribe().await;

            let mut initial_views = Vec::new();
            for i in items {
                let mapped = map_timeline_item_safe(&client, &i).await;
                actor
                    .search_engine
                    .lock()
                    .await
                    .inner
                    .index_item(&args.room_id, &mapped);
                initial_views.push(mapped);
            }

            actor.send_event(ToShell::Room(RoomEvents::TimelineDiff(TimelineDiffArgs {
                room_id: args.room_id.clone(),
                diff: vec![TimelineDiff::Reset {
                    entries: initial_views,
                }],
            })));

            actor
                .active_timelines
                .borrow_mut()
                .insert(args.room_id.clone(), std::rc::Rc::new(timeline));

            let sender = actor.event_sender.clone();
            let stream_room_id = args.room_id.clone();
            let search_engine = actor.search_engine.clone();
            let mapper_client = client.clone();

            spawn_local(async move {
                while let Some(diffs) = stream.next().await {
                    let mut mapped_diffs = Vec::new();
                    for diff in diffs {
                        let mapped_diff = map_timeline_diff(&mapper_client, diff).await;

                        match &mapped_diff {
                            TimelineDiff::Append { entries } | TimelineDiff::Reset { entries } => {
                                let search_engine = search_engine.clone();
                                let stream_room_id = stream_room_id.clone();
                                let entries = entries.clone();

                                spawn_local(async move {
                                    for entry in &entries {
                                        search_engine
                                            .lock()
                                            .await
                                            .upsert_live_event(&stream_room_id, entry)
                                            .await;
                                    }
                                });
                            }
                            TimelineDiff::PushFront { entry }
                            | TimelineDiff::PushBack { entry }
                            | TimelineDiff::Insert { entry, .. }
                            | TimelineDiff::Set { entry, .. } => {
                                let search_engine = search_engine.clone();
                                let stream_room_id = stream_room_id.clone();
                                let entry = entry.clone();

                                spawn_local(async move {
                                    search_engine
                                        .lock()
                                        .await
                                        .upsert_live_event(&stream_room_id, &entry)
                                        .await;
                                });
                            }
                            #[allow(clippy::match_same_arms)]
                            TimelineDiff::Remove { index: _ } => {}
                            _ => {}
                        }

                        mapped_diffs.push(mapped_diff);
                    }
                    let _ = sender.unbounded_send(ToShell::Room(RoomEvents::TimelineDiff(
                        TimelineDiffArgs {
                            room_id: stream_room_id.clone(),
                            diff: mapped_diffs,
                        },
                    )));
                }
            });
        }
    }
    vec![]
}
