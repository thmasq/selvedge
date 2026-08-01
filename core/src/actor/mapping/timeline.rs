use eyeball_im::VectorDiff;
use matrix_sdk::Client;
use matrix_sdk_ui::timeline::{TimelineItemKind, VirtualTimelineItem};
use selvedge_shared::{TimelineDiff, TimelineItem, VirtualItem};
use std::sync::Arc;

use super::event::map_event_item;

pub const fn map_virtual_item(virt: &VirtualTimelineItem) -> VirtualItem {
    match virt {
        VirtualTimelineItem::DateDivider(ts) => VirtualItem::DayDivider {
            ts: matrix_sdk::ruma::MilliSecondsSinceUnixEpoch(ts.0),
        },
        _ => VirtualItem::LoadingIndicator,
    }
}

pub async fn map_timeline_item_safe(
    client: &Client,
    item: &matrix_sdk_ui::timeline::TimelineItem,
) -> TimelineItem {
    match item.kind() {
        TimelineItemKind::Event(event) => {
            TimelineItem::Event(Box::new(map_event_item(client, event).await))
        }
        TimelineItemKind::Virtual(virt) => TimelineItem::Virtual(map_virtual_item(virt)),
    }
}

pub async fn map_timeline_diff(
    client: &Client,
    diff: VectorDiff<Arc<matrix_sdk_ui::timeline::TimelineItem>>,
) -> TimelineDiff {
    match diff {
        VectorDiff::Append { values } => {
            let mut entries = Vec::new();
            for v in values {
                entries.push(map_timeline_item_safe(client, &v).await);
            }
            TimelineDiff::Append { entries }
        }
        VectorDiff::Clear => TimelineDiff::Clear,
        VectorDiff::PushFront { value } => TimelineDiff::PushFront {
            entry: map_timeline_item_safe(client, &value).await,
        },
        VectorDiff::PushBack { value } => TimelineDiff::PushBack {
            entry: map_timeline_item_safe(client, &value).await,
        },
        VectorDiff::PopFront => TimelineDiff::PopFront,
        VectorDiff::PopBack => TimelineDiff::PopBack,
        VectorDiff::Insert { index, value } => TimelineDiff::Insert {
            index,
            entry: map_timeline_item_safe(client, &value).await,
        },
        VectorDiff::Set { index, value } => TimelineDiff::Set {
            index,
            entry: map_timeline_item_safe(client, &value).await,
        },
        VectorDiff::Remove { index } => TimelineDiff::Remove { index },
        VectorDiff::Truncate { length } => TimelineDiff::Truncate { length },
        VectorDiff::Reset { values } => {
            let mut entries = Vec::new();
            for v in values {
                entries.push(map_timeline_item_safe(client, &v).await);
            }
            TimelineDiff::Reset { entries }
        }
    }
}
