use super::super::MatrixActor;
use super::super::mapping::map_room_list_diff;
use futures::StreamExt;
use matrix_sdk::Client;
use matrix_sdk::ruma::events::ToDeviceEvent;
use matrix_sdk::ruma::events::key::verification::done::ToDeviceKeyVerificationDoneEventContent;
use matrix_sdk::ruma::events::key::verification::request::ToDeviceKeyVerificationRequestEventContent;
use matrix_sdk::ruma::events::room_key_request::{Action, ToDeviceRoomKeyRequestEventContent};
use matrix_sdk_ui::room_list_service::{RoomListService, filters::new_filter_all};
use selvedge_shared::{ActorError, VerificationState, message::ToShell};
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

impl MatrixActor {
    pub(crate) fn start_sync(&self) {
        let client = self.client.borrow().clone();
        if let Some(client) = client {
            let sender = self.event_sender.clone();

            let verification_sender = sender.clone();
            let verification_done_sender = sender.clone();
            let room_key_request_sender = sender.clone();

            client.add_event_handler(
                move |ev: ToDeviceEvent<ToDeviceKeyVerificationRequestEventContent>,
                      _client: Client| {
                    let sender_for_async = verification_sender.clone();
                    async move {
                        let user_id = ev.sender.clone();
                        let flow_id = ev.content.transaction_id.to_string();

                        if let Some(request) = _client
                            .encryption()
                            .get_verification_request(&user_id, &flow_id)
                            .await
                        {
                            if request.is_cancelled() || request.is_done() {
                                return;
                            }

                            let _ = sender_for_async.unbounded_send(ToShell::VerificationUpdate {
                                user_id,
                                flow_id,
                                state: VerificationState::Requested {
                                    methods: ev.content.methods.clone(),
                                },
                            });
                        }
                    }
                },
            );

            client.add_event_handler(
                move |ev: ToDeviceEvent<ToDeviceKeyVerificationDoneEventContent>,
                      event_client: Client| {
                    let sender_for_async = verification_done_sender.clone();
                    async move {
                        let is_verified = event_client
                            .encryption()
                            .get_user_identity(&ev.sender)
                            .await
                            .ok()
                            .flatten()
                            .is_some_and(|id| id.is_verified());

                        let _ = sender_for_async.unbounded_send(ToShell::IdentityUpdated {
                            user_id: ev.sender,
                            is_verified,
                        });
                    }
                },
            );

            client.add_event_handler(
                move |ev: ToDeviceEvent<ToDeviceRoomKeyRequestEventContent>, _client: Client| {
                    let sender_for_async = room_key_request_sender.clone();
                    async move {
                        if matches!(ev.content.action, Action::Request) {
                            let _ =
                                sender_for_async.unbounded_send(ToShell::RoomKeyRequestReceived {
                                    request_id: ev.content.request_id.to_string(),
                                    requester_user_id: ev.sender.clone(),
                                    requester_device_id: ev
                                        .content
                                        .requesting_device_id
                                        .to_string(),
                                });
                        }
                    }
                },
            );

            spawn_local(async move {
                match RoomListService::new(client.clone()).await {
                    Ok(room_list_service) => {
                        let room_list_service = Rc::new(room_list_service);

                        {
                            let svc = room_list_service.clone();
                            spawn_local(async move {
                                let sync_stream = svc.sync();
                                futures::pin_mut!(sync_stream);
                                while sync_stream.next().await.is_some() {}
                            });
                        }

                        {
                            let svc = room_list_service;
                            let sender = sender.clone();
                            let mapper_client = client.clone();

                            spawn_local(async move {
                                if let Ok(all_rooms) = svc.all_rooms().await {
                                    let (entries_stream, controller) =
                                        all_rooms.entries_with_dynamic_adapters(50);

                                    controller.set_filter(Box::new(new_filter_all(vec![])));

                                    futures::pin_mut!(entries_stream);

                                    while let Some(diffs) = entries_stream.next().await {
                                        let mapped = futures::future::join_all(
                                            diffs.into_iter().map(|diff| {
                                                map_room_list_diff(mapper_client.clone(), diff)
                                            }),
                                        )
                                        .await;

                                        let _ =
                                            sender.unbounded_send(ToShell::RoomListDiff(mapped));
                                    }
                                }
                            });
                        }
                    }
                    Err(e) => {
                        let _ = sender.unbounded_send(ToShell::BackgroundError(
                            ActorError::SyncInitializationFailed(e.to_string()),
                        ));
                    }
                }
            });
        }
    }
}
