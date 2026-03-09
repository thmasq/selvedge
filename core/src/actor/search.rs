use matrix_sdk::ruma::{OwnedEventId, OwnedRoomId};
use selvedge_shared::{EventItem, MessageContent, TimelineContent, TimelineItem};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub(crate) struct SearchIndex {
    word_to_events: HashMap<String, HashSet<OwnedEventId>>,
    event_store: HashMap<OwnedEventId, (OwnedRoomId, EventItem)>,
}

impl SearchIndex {
    pub(crate) fn index_item(&mut self, room_id: &OwnedRoomId, item: &TimelineItem) {
        if let TimelineItem::Event(event_item) = item {
            if let TimelineContent::Message(MessageContent::Text { body, .. }) =
                &*event_item.content
            {
                let tokens: Vec<String> = body
                    .to_lowercase()
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();

                for token in tokens {
                    self.word_to_events
                        .entry(token)
                        .or_default()
                        .insert(event_item.event_id.clone());
                }

                self.event_store.insert(
                    event_item.event_id.clone(),
                    (room_id.clone(), event_item.clone()),
                );
            }
        }
    }

    pub(crate) fn search(
        &self,
        room_id_filter: Option<&OwnedRoomId>,
        query: &str,
        limit: usize,
    ) -> Vec<EventItem> {
        let tokens: Vec<String> = query
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        if tokens.is_empty() {
            return vec![];
        }

        let mut result_sets: Vec<&HashSet<OwnedEventId>> = Vec::new();
        for token in &tokens {
            if let Some(set) = self.word_to_events.get(token) {
                result_sets.push(set);
            } else {
                return vec![];
            }
        }

        if result_sets.is_empty() {
            return vec![];
        }

        let mut intersection = result_sets[0].clone();
        for set in result_sets.into_iter().skip(1) {
            intersection.retain(|id| set.contains(id));
        }

        let mut matched_events: Vec<EventItem> = intersection
            .into_iter()
            .filter_map(|id| self.event_store.get(&id))
            .filter(|(r_id, _)| room_id_filter.map_or(true, |f| f == r_id))
            .map(|(_, item)| item.clone())
            .collect();

        matched_events.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        matched_events.into_iter().take(limit).collect()
    }
}
