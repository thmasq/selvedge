use super::super::MatrixActor;
use super::super::message::ToShell;
use crate::model::ActorError;
use matrix_sdk::attachment::AttachmentConfig;
use matrix_sdk::ruma::OwnedRoomId;
use matrix_sdk::ruma::api::client::filter::RoomEventFilter;
use matrix_sdk::ruma::api::client::search::search_events::v3::{
    Categories, Criteria, Request as SearchRequest,
};
use matrix_sdk::ruma::events::room::message::RoomMessageEventContent;
use std::str::FromStr;

impl MatrixActor {
    #[allow(clippy::future_not_send)]
    pub(crate) async fn send_message(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
        body: String,
    ) -> Vec<ToShell> {
        let timeline = self.active_timelines.borrow().get(&room_id).cloned();

        let result = if let Some(timeline) = timeline {
            let content = RoomMessageEventContent::text_plain(body);
            timeline
                .send(content.into())
                .await
                .map(|_| ())
                .map_err(|_e| ActorError::RoomOperationFailed("Timeline send failed".to_string()))
        } else {
            let room = self
                .client
                .borrow()
                .as_ref()
                .and_then(|c| c.get_room(&room_id));

            if let Some(room) = room {
                let content = RoomMessageEventContent::text_plain(body);
                room.send(content)
                    .await
                    .map(|_| ())
                    .map_err(|e| ActorError::RoomOperationFailed(e.to_string()))
            } else {
                Err(ActorError::ClientNotInitialized)
            }
        };

        match result {
            Ok(_) => vec![ToShell::CommandResult {
                request_id,
                success: true,
                error: None,
            }],
            Err(e) => vec![ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(e),
            }],
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn send_media(
        &self,
        request_id: String,
        room_id: OwnedRoomId,
        filename: String,
        mime_type: String,
        data: Vec<u8>,
    ) -> Vec<ToShell> {
        let room = self
            .client
            .borrow()
            .as_ref()
            .and_then(|c| c.get_room(&room_id));

        let result = if let Some(room) = room {
            let config = AttachmentConfig::new();
            if let Ok(mime) = mime::Mime::from_str(&mime_type) {
                room.send_attachment(&filename, &mime, data, config)
                    .await
                    .map(|_| ())
                    .map_err(|e| ActorError::RoomOperationFailed(e.to_string()))
            } else {
                Err(ActorError::RoomOperationFailed(
                    "Invalid MIME type".to_string(),
                ))
            }
        } else {
            Err(ActorError::ClientNotInitialized)
        };

        match result {
            Ok(_) => vec![ToShell::CommandResult {
                request_id,
                success: true,
                error: None,
            }],
            Err(e) => vec![ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(e),
            }],
        }
    }

    #[allow(clippy::future_not_send)]
    pub(crate) async fn search_messages(
        &self,
        request_id: String,
        room_id: Option<OwnedRoomId>,
        query: String,
        limit: usize,
    ) -> Vec<ToShell> {
        let client_opt = self.client.borrow().clone();
        let search_index = self.search_index.clone();

        let mut server_results: Option<Vec<crate::model::EventItem>> = None;

        if let (Some(client), Some(r_id)) = (&client_opt, &room_id) {
            if let Some(room) = client.get_room(r_id) {
                let is_encrypted = room.encryption_state().is_encrypted();

                if !is_encrypted {
                    let mut filter = RoomEventFilter::default();
                    filter.rooms = Some(vec![r_id.clone()]);
                    filter.limit = Some(limit.try_into().unwrap_or(20_u32).into());

                    let mut criteria = Criteria::new(query.clone());
                    criteria.filter = filter;

                    let mut categories = Categories::new();
                    categories.room_events = Some(criteria);

                    let request = SearchRequest::new(categories);

                    if let Ok(response) = client.send(request).await {
                        let mut items = Vec::new();

                        for result in response.search_categories.room_events.results {
                            if let Some(raw_event) = result.result {
                                if let Ok(Some(event_id)) = raw_event
                                    .get_field::<matrix_sdk::ruma::OwnedEventId>("event_id")
                                {
                                    if let Ok(Some(sender)) = raw_event
                                        .get_field::<matrix_sdk::ruma::OwnedUserId>("sender")
                                    {
                                        let timestamp = raw_event
                                            .get_field::<matrix_sdk::ruma::MilliSecondsSinceUnixEpoch>(
                                                "origin_server_ts",
                                            )
                                            .ok()
                                            .flatten()
                                            .unwrap_or_else(|| {
                                                matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(
                                                    0u32.into(),
                                                )
                                            });

                                        items.push(crate::model::EventItem {
                                            event_id,
                                            sender,
                                            sender_profile: None,
                                            timestamp,
                                            content: Box::new(
                                                crate::model::TimelineContent::Unsupported,
                                            ),
                                            reactions: indexmap::IndexMap::new(),
                                            read_receipts: Vec::new(),
                                            delivery_status: crate::model::DeliveryStatus::Synced,
                                            in_reply_to: None,
                                            reply_details: None,
                                            is_edited: false,
                                            latest_edit: None,
                                            thread_root_id: None,
                                            is_highlight: false,
                                            should_group: false,
                                            encryption_status:
                                                crate::model::EncryptionStatus::Unencrypted,
                                        });
                                    }
                                }
                            }
                        }

                        server_results = Some(items);
                    }
                }
            }
        }

        let results = server_results.unwrap_or_else(|| {
            search_index
                .borrow()
                .search(room_id.as_ref(), &query, limit)
        });

        vec![ToShell::SearchResults {
            request_id,
            results,
        }]
    }
}
