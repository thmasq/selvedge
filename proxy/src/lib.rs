#![allow(clippy::future_not_send)]

use matrix_sdk::Client;
use matrix_sdk::cross_process_lock::CrossProcessLockConfig;
use matrix_sdk_ui::notification_client::{
    NotificationClient, NotificationProcessSetup, NotificationStatus,
};
use ruma::{OwnedEventId, OwnedRoomId};
use serde::Deserialize;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{NotificationOptions, PushEvent, ServiceWorkerGlobalScope};

#[derive(Deserialize)]
struct MatrixPushPayload {
    room_id: OwnedRoomId,
    event_id: OwnedEventId,
}

#[wasm_bindgen(start)]
pub fn main_js() {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));
}

#[wasm_bindgen]
pub async fn handle_push(event: PushEvent) {
    let sw = js_sys::global().unchecked_into::<ServiceWorkerGlobalScope>();

    let clients_promise = sw.clients().match_all();

    if let Ok(clients_val) = JsFuture::from(clients_promise).await {
        let clients_array: js_sys::Array = clients_val.into();
        if clients_array.length() > 0 {
            return;
        }
    }

    let Some(data) = event.data() else { return };
    let Ok(payload) = serde_json::from_str::<MatrixPushPayload>(&data.text()) else {
        return;
    };

    let Ok(parent_client) = Client::builder()
        .indexeddb_store("selvedge-store", None)
        .cross_process_store_config(CrossProcessLockConfig::multi_process("selvedge_app"))
        .build()
        .await
    else {
        return;
    };

    let Ok(notification_client) =
        NotificationClient::new(parent_client, NotificationProcessSetup::MultipleProcesses).await
    else {
        return;
    };

    if let Ok(NotificationStatus::Event(item)) = notification_client
        .get_notification(&payload.room_id, &payload.event_id)
        .await
    {
        let options = NotificationOptions::new();
        options.set_body(&format!(
            "New message in {}",
            item.room_computed_display_name
        ));

        if let Some(avatar) = item.sender_avatar_url {
            options.set_icon(&avatar);
        }

        let title = item
            .sender_display_name
            .unwrap_or_else(|| "Someone".to_string());

        let _ = sw
            .registration()
            .show_notification_with_options(&title, &options);
    }
}

#[wasm_bindgen]
#[must_use]
pub fn rewrite_proxy_url(original_url: &str, proxy_server: &str) -> String {
    let clean_server = proxy_server
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    format!("https://{clean_server}/proxy/{original_url}")
}
