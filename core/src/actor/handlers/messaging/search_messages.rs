use crate::actor::MatrixActor;
use matrix_sdk::ruma::api::client::filter::RoomEventFilter;
use matrix_sdk::ruma::api::client::search::search_events::v3::{
    Categories, Criteria, Request as SearchRequest,
};
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::search_results::SearchResultsArgs;
use selvedge_shared::message::messaging::search_messages::SearchMessagesArgs;
use selvedge_shared::model::{DeliveryStatus, EncryptionStatus, EventItem, TimelineContent};

pub async fn run(actor: &MatrixActor, args: SearchMessagesArgs) -> Vec<ToShell> {
    let client_opt = actor.client.borrow().clone();
    let search_engine = actor.search_engine.clone();

    let mut server_results: Option<Vec<EventItem>> = None;

    if let (Some(client), Some(r_id)) = (&client_opt, &args.room_id)
        && let Some(room) = client.get_room(r_id)
    {
        let is_encrypted = room.encryption_state().is_encrypted();

        if !is_encrypted {
            let mut filter = RoomEventFilter::default();
            filter.rooms = Some(vec![r_id.clone()]);
            filter.limit = Some(args.limit.try_into().unwrap_or(20_u32).into());

            let mut criteria = Criteria::new(args.query.clone());
            criteria.filter = filter;

            let mut categories = Categories::new();
            categories.room_events = Some(criteria);

            let request = SearchRequest::new(categories);

            if let Ok(response) = client.send(request).await {
                let mut items = Vec::new();

                for result in response.search_categories.room_events.results {
                    if let Some(raw_event) = result.result
                        && let Ok(Some(event_id)) =
                            raw_event.get_field::<matrix_sdk::ruma::OwnedEventId>("event_id")
                        && let Ok(Some(sender)) =
                            raw_event.get_field::<matrix_sdk::ruma::OwnedUserId>("sender")
                    {
                        let timestamp = raw_event
                            .get_field::<matrix_sdk::ruma::MilliSecondsSinceUnixEpoch>(
                                "origin_server_ts",
                            )
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| {
                                matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(0u32.into())
                            });

                        let content = if let Ok(Some(msg_content)) =
                            raw_event.get_field::<RoomMessageEventContent>("content")
                        {
                            Box::new(msg_content.into())
                        } else {
                            Box::new(TimelineContent::Unsupported)
                        };

                        items.push(EventItem {
                            event_id,
                            sender,
                            sender_profile: None,
                            timestamp,
                            content,
                            reactions: indexmap::IndexMap::new(),
                            read_receipts: Vec::new(),
                            delivery_status: DeliveryStatus::Synced,
                            in_reply_to: None,
                            reply_details: None,
                            is_edited: false,
                            latest_edit: None,
                            thread_root_id: None,
                            is_own_mention: false,
                            is_highlight: false,
                            is_trusted: true,
                            should_group: false,
                            encryption_status: EncryptionStatus::Unencrypted,
                        });
                    }
                }

                server_results = Some(items);
            }
        }
    }

    let local_results =
        search_engine
            .lock()
            .await
            .inner
            .search(args.room_id.as_ref(), &args.query, args.limit);

    let mut results = local_results;

    if let Some(server) = server_results {
        let mut seen_ids: std::collections::HashSet<_> =
            results.iter().map(|item| item.event_id.clone()).collect();

        for srv_item in server {
            if !seen_ids.contains(&srv_item.event_id) {
                seen_ids.insert(srv_item.event_id.clone());
                results.push(srv_item);
            }
        }
    }

    vec![ToShell::Core(CoreEvents::SearchResults(
        SearchResultsArgs {
            request_id: args.request_id,
            results,
        },
    ))]
}
