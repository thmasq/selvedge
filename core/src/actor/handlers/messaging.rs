use super::super::MatrixActor;
use matrix_sdk::attachment::AttachmentConfig;
use matrix_sdk::media::{MediaFormat, MediaRequestParameters};
use matrix_sdk::ruma::OwnedRoomId;
use matrix_sdk::ruma::api::client::filter::RoomEventFilter;
use matrix_sdk::ruma::api::client::search::search_events::v3::{
    Categories, Criteria, Request as SearchRequest,
};
use matrix_sdk::ruma::events::room::message::{
    AddMentions, ForwardThread, RoomMessageEventContent,
};
use matrix_sdk::ruma::events::{AnyMessageLikeEvent, AnyTimelineEvent, MessageLikeEvent};
use pulldown_cmark::{Options, Parser, html};
use selvedge_shared::{
    ActorError, DeliveryStatus, EncryptionStatus, EventItem, TimelineContent, message::ToShell,
    model::MediaSource,
};
use std::str::FromStr;

impl MatrixActor {
    #[allow(clippy::future_not_send)]
    pub(crate) async fn send_message(
        &self,
        request_id: String,
        room_id: matrix_sdk::ruma::OwnedRoomId,
        body: String,
        reply_to: Option<matrix_sdk::ruma::OwnedEventId>,
    ) -> Vec<ToShell> {
        let timeline = self.active_timelines.borrow().get(&room_id).cloned();

        let room = self
            .client
            .borrow()
            .as_ref()
            .and_then(|c| c.get_room(&room_id));

        let result = if let Some(room) = room {
            let mut options = Options::empty();
            options.insert(Options::ENABLE_STRIKETHROUGH);
            options.insert(Options::ENABLE_TABLES);

            let parser = Parser::new_ext(&body, options);
            let mut raw_html = String::new();
            html::push_html(&mut raw_html, parser);

            let safe_html = selvedge_shared::sanitize_matrix_html(&raw_html);
            let trimmed_html = safe_html.trim().to_string();

            let mut content = RoomMessageEventContent::text_html(body.clone(), trimmed_html);

            if let Some(event_id) = reply_to {
                if let Ok(event) = room.event(&event_id, None).await {
                    if let Ok(any_event) = event.kind.into_raw().deserialize() {
                        let full_event = any_event.into_full_event(room_id.clone());

                        if let AnyTimelineEvent::MessageLike(AnyMessageLikeEvent::RoomMessage(
                            MessageLikeEvent::Original(orig_msg),
                        )) = full_event
                        {
                            content = content.make_reply_to(
                                &orig_msg,
                                ForwardThread::Yes,
                                AddMentions::Yes,
                            );
                        }
                    }
                }
            }

            if let Some(timeline) = timeline {
                timeline
                    .send(content.into())
                    .await
                    .map(|_| ())
                    .map_err(|_e| {
                        ActorError::RoomOperationFailed("Timeline send failed".to_string())
                    })
            } else {
                room.send(content)
                    .await
                    .map(|_| ())
                    .map_err(|e| ActorError::RoomOperationFailed(e.to_string()))
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

        let mut server_results: Option<Vec<EventItem>> = None;

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

                                        items.push(EventItem {
                                            event_id,
                                            sender,
                                            sender_profile: None,
                                            timestamp,
                                            content: Box::new(TimelineContent::Unsupported),
                                            reactions: indexmap::IndexMap::new(),
                                            read_receipts: Vec::new(),
                                            delivery_status: DeliveryStatus::Synced,
                                            in_reply_to: None,
                                            reply_details: None,
                                            is_edited: false,
                                            latest_edit: None,
                                            thread_root_id: None,
                                            is_highlight: false,
                                            should_group: false,
                                            encryption_status: EncryptionStatus::Unencrypted,
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

    #[allow(clippy::future_not_send)]
    pub(crate) async fn fetch_and_decrypt_media(
        &self,
        request_id: String,
        source: MediaSource,
        mime_type: String,
    ) -> Vec<ToShell> {
        let client = self.client.borrow().clone();

        if let Some(client) = client {
            let ruma_source = match source {
                MediaSource::Plain(uri) => matrix_sdk::ruma::events::room::MediaSource::Plain(uri),
                MediaSource::Encrypted(file) => {
                    matrix_sdk::ruma::events::room::MediaSource::Encrypted(Box::new(*file))
                }
            };

            let request = MediaRequestParameters {
                source: ruma_source,
                format: MediaFormat::File,
            };

            match client.media().get_media_content(&request, true).await {
                Ok(data) => vec![ToShell::MediaDecrypted {
                    request_id,
                    mime_type,
                    data,
                }],
                Err(e) => vec![ToShell::CommandResult {
                    request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(e.to_string())),
                }],
            }
        } else {
            vec![ToShell::CommandResult {
                request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            }]
        }
    }
}
