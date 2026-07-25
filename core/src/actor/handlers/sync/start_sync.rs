use crate::actor::MatrixActor;
use crate::actor::mapping::map_room_list_diff;
use futures::StreamExt;
use matrix_sdk::ruma::events::ToDeviceEvent;
use matrix_sdk::ruma::events::key::verification::done::ToDeviceKeyVerificationDoneEventContent;
use matrix_sdk::ruma::events::key::verification::request::ToDeviceKeyVerificationRequestEventContent;
use matrix_sdk::ruma::events::receipt::{ReceiptType, SyncReceiptEvent};
use matrix_sdk::ruma::events::room_key_request::{Action, ToDeviceRoomKeyRequestEventContent};
use matrix_sdk::ruma::events::typing::SyncTypingEvent;
use matrix_sdk::ruma::presence::PresenceState;
use matrix_sdk::{Client, Room};
use matrix_sdk_ui::room_list_service::{RoomListService, filters::new_filter_all};

use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::background_error::BackgroundErrorArgs;
use selvedge_shared::event::crypto::CryptoEvents;
use selvedge_shared::event::crypto::identity_updated::IdentityUpdatedArgs;
use selvedge_shared::event::crypto::room_key_request_received::RoomKeyRequestReceivedArgs;
use selvedge_shared::event::crypto::verification_update::VerificationUpdateArgs;
use selvedge_shared::event::room::RoomEvents;
use selvedge_shared::event::room::profiles_fetched::ProfilesFetchedArgs;
use selvedge_shared::event::room::room_list_diff::RoomListDiffArgs;
use selvedge_shared::event::room::typing_updated::TypingUpdatedArgs;
use selvedge_shared::message::sync::start_sync::StartSyncArgs;
use selvedge_shared::{MemberProfile, model::ActorError, model::VerificationState};

use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen_futures::spawn_local;

pub async fn run(actor: &MatrixActor, _args: StartSyncArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();
    if let Some(client) = client {
        let sender = actor.event_sender.clone();

        let verification_sender = sender.clone();
        let verification_done_sender = sender.clone();
        let room_key_request_sender = sender.clone();
        let typing_sender = sender.clone();
        let receipt_sender = sender.clone();

        client.add_event_handler(
            move |ev: SyncTypingEvent, room: Room, event_client: Client| {
                let sender_for_async = typing_sender.clone();
                async move {
                    let room_id = room.room_id().to_owned();
                    let typing_users = ev.content.user_ids.clone();

                    let _ = sender_for_async.unbounded_send(ToShell::Room(
                        RoomEvents::TypingUpdated(TypingUpdatedArgs {
                            room_id: room_id.clone(),
                            typing_users: typing_users.clone(),
                        }),
                    ));

                    let mut profiles = HashMap::new();
                    for user_id in typing_users {
                        if let Ok(Some(member)) = room.get_member(&user_id).await {
                            let is_verified = event_client
                                .encryption()
                                .get_user_identity(&user_id)
                                .await
                                .ok()
                                .flatten()
                                .is_some_and(|id| id.is_verified());

                            profiles.insert(
                                user_id.clone(),
                                MemberProfile {
                                    user_id: user_id.clone(),
                                    display_name: member.display_name().map(ToOwned::to_owned),
                                    avatar_url: member.avatar_url().map(ToOwned::to_owned),
                                    membership: member.membership().clone(),
                                    presence: PresenceState::Offline,
                                    is_verified,
                                },
                            );
                        }
                    }

                    if !profiles.is_empty() {
                        let _ = sender_for_async.unbounded_send(ToShell::Room(
                            RoomEvents::ProfilesFetched(ProfilesFetchedArgs { room_id, profiles }),
                        ));
                    }
                }
            },
        );

        client.add_event_handler(
            move |ev: SyncReceiptEvent, room: Room, event_client: Client| {
                let sender_for_async = receipt_sender.clone();
                async move {
                    let room_id = room.room_id().to_owned();
                    let mut profiles = HashMap::new();

                    for (_event_id, receipts) in ev.content.0 {
                        if let Some(read_receipts) = receipts.get(&ReceiptType::Read) {
                            for user_id in read_receipts.keys() {
                                if let Ok(Some(member)) = room.get_member(user_id).await {
                                    let is_verified = event_client
                                        .encryption()
                                        .get_user_identity(user_id)
                                        .await
                                        .ok()
                                        .flatten()
                                        .is_some_and(|id| id.is_verified());

                                    profiles.insert(
                                        user_id.clone(),
                                        MemberProfile {
                                            user_id: user_id.clone(),
                                            display_name: member
                                                .display_name()
                                                .map(ToOwned::to_owned),
                                            avatar_url: member.avatar_url().map(ToOwned::to_owned),
                                            membership: member.membership().clone(),
                                            presence: PresenceState::Offline,
                                            is_verified,
                                        },
                                    );
                                }
                            }
                        }
                    }

                    if !profiles.is_empty() {
                        let _ = sender_for_async.unbounded_send(ToShell::Room(
                            RoomEvents::ProfilesFetched(ProfilesFetchedArgs { room_id, profiles }),
                        ));
                    }
                }
            },
        );

        client.add_event_handler(
            move |ev: ToDeviceEvent<ToDeviceKeyVerificationRequestEventContent>, client: Client| {
                let sender_for_async = verification_sender.clone();
                async move {
                    let user_id = ev.sender.clone();
                    let flow_id = ev.content.transaction_id.to_string();

                    if let Some(request) = client
                        .encryption()
                        .get_verification_request(&user_id, &flow_id)
                        .await
                    {
                        if request.is_cancelled() || request.is_done() {
                            return;
                        }

                        let _ = sender_for_async.unbounded_send(ToShell::Crypto(
                            CryptoEvents::VerificationUpdate(VerificationUpdateArgs {
                                user_id,
                                flow_id,
                                state: VerificationState::Requested {
                                    methods: ev.content.methods.clone(),
                                },
                            }),
                        ));
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

                    let _ = sender_for_async.unbounded_send(ToShell::Crypto(
                        CryptoEvents::IdentityUpdated(IdentityUpdatedArgs {
                            user_id: ev.sender,
                            is_verified,
                        }),
                    ));
                }
            },
        );

        client.add_event_handler(
            move |ev: ToDeviceEvent<ToDeviceRoomKeyRequestEventContent>, _client: Client| {
                let sender_for_async = room_key_request_sender.clone();
                async move {
                    if matches!(ev.content.action, Action::Request) {
                        let _ = sender_for_async.unbounded_send(ToShell::Crypto(
                            CryptoEvents::RoomKeyRequestReceived(RoomKeyRequestReceivedArgs {
                                request_id: ev.content.request_id.to_string(),
                                requester_user_id: ev.sender.clone(),
                                requester_device_id: ev.content.requesting_device_id.to_string(),
                            }),
                        ));
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
                                    let mapped =
                                        futures::future::join_all(diffs.into_iter().map(|diff| {
                                            map_room_list_diff(mapper_client.clone(), diff)
                                        }))
                                        .await;

                                    let _ = sender.unbounded_send(ToShell::Room(
                                        RoomEvents::RoomListDiff(RoomListDiffArgs { diff: mapped }),
                                    ));
                                }
                            }
                        });
                    }
                }
                Err(e) => {
                    let _ = sender.unbounded_send(ToShell::Core(CoreEvents::BackgroundError(
                        BackgroundErrorArgs {
                            error: ActorError::SyncInitializationFailed(e.to_string()),
                        },
                    )));
                }
            }
        });
    }
    vec![]
}
