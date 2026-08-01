use eyeball_im::VectorDiff;
use matrix_sdk::Client;
use matrix_sdk::ruma::MilliSecondsSinceUnixEpoch;
use matrix_sdk_ui::room_list_service::RoomListItem;
use selvedge_shared::{RoomListEntryDiff, RoomListEntryView, RoomSummary};
use std::collections::HashSet;

pub fn compute_last_activity(client: &Client, item: &RoomListItem) -> MilliSecondsSinceUnixEpoch {
    client
        .get_room(item.room_id())
        .and_then(|room| room.latest_event_timestamp())
        .unwrap_or_else(|| MilliSecondsSinceUnixEpoch(0u32.into()))
}

pub fn is_room_encrypted(room: Option<&matrix_sdk::Room>) -> bool {
    room.as_ref()
        .is_some_and(|r| r.encryption_state().is_encrypted())
}

pub async fn build_room_summary(client: &Client, item: &RoomListItem) -> RoomSummary {
    let unread = item.unread_notification_counts();
    let last_activity = compute_last_activity(client, item);
    let room = client.get_room(item.room_id());
    let receipts = item.read_receipts();
    let is_encrypted = is_room_encrypted(room.as_ref());

    RoomSummary {
        room_id: item.room_id().to_owned(),
        name: item.name(),
        avatar_url: item.avatar_url(),
        notification_count: unread.notification_count,
        highlight_count: unread.highlight_count,
        unread_count: receipts.num_unread,
        is_direct: item.is_direct().await.unwrap_or(false),
        last_message_preview: None,
        last_activity,
        has_active_call: false,
        active_call_participant_count: 0,
        is_encrypted,
        tags: HashSet::new(),
    }
}

#[allow(clippy::future_not_send)]
pub async fn room_list_item_to_view(client: Client, item: RoomListItem) -> RoomListEntryView {
    RoomListEntryView::Filled(build_room_summary(&client, &item).await)
}

#[allow(clippy::future_not_send)]
pub async fn map_room_list_diff(
    client: Client,
    diff: VectorDiff<RoomListItem>,
) -> RoomListEntryDiff {
    match diff {
        VectorDiff::Append { values } => RoomListEntryDiff::Append {
            entries: futures::future::join_all(
                values
                    .into_iter()
                    .map(|v| room_list_item_to_view(client.clone(), v)),
            )
            .await,
        },
        VectorDiff::Clear => RoomListEntryDiff::Clear,
        VectorDiff::PushFront { value } => RoomListEntryDiff::PushFront {
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::PushBack { value } => RoomListEntryDiff::PushBack {
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::PopFront => RoomListEntryDiff::PopFront,
        VectorDiff::PopBack => RoomListEntryDiff::PopBack,
        VectorDiff::Insert { index, value } => RoomListEntryDiff::Insert {
            index,
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::Set { index, value } => RoomListEntryDiff::Set {
            index,
            entry: room_list_item_to_view(client, value).await,
        },
        VectorDiff::Remove { index } => RoomListEntryDiff::Remove { index },
        VectorDiff::Truncate { length } => RoomListEntryDiff::Truncate { length },
        VectorDiff::Reset { values } => RoomListEntryDiff::Reset {
            entries: futures::future::join_all(
                values
                    .into_iter()
                    .map(|v| room_list_item_to_view(client.clone(), v)),
            )
            .await,
        },
    }
}
