use matrix_sdk::ruma::events::{StateEventContentChange, room::member::MembershipState};
use matrix_sdk_ui::timeline::{
    AnyOtherStateEventContentChange, MemberProfileChange, OtherState, RoomMembershipChange,
};
use selvedge_shared::model::StateContent;

pub fn map_membership(change: &RoomMembershipChange) -> StateContent {
    StateContent::Member {
        user_id: change.user_id().to_owned(),
        membership: MembershipState::Join, // Optional: Extract deeply from change.content()
        prev_membership: None,
        reason: None,
        change: change.change().map(|c| format!("{c:?}")),
    }
}

pub fn map_profile(change: &MemberProfileChange) -> StateContent {
    StateContent::ProfileChange {
        user_id: change.user_id().to_owned(),
        displayname_change: change
            .displayname_change()
            .map(|c| (c.old.clone(), c.new.clone())),
        avatar_url_change: change
            .avatar_url_change()
            .map(|c| (c.old.clone(), c.new.clone())),
    }
}

pub fn map_other(state: &OtherState) -> StateContent {
    match state.content() {
        AnyOtherStateEventContentChange::RoomName(n) => StateContent::RoomName {
            name: match n {
                StateEventContentChange::Original { content, .. } => Some(content.name.clone()),
                StateEventContentChange::Redacted(_) => None,
            },
        },
        AnyOtherStateEventContentChange::RoomTopic(t) => StateContent::RoomTopic {
            topic: match t {
                StateEventContentChange::Original { content, .. } => Some(content.topic.clone()),
                StateEventContentChange::Redacted(_) => None,
            },
        },
        AnyOtherStateEventContentChange::RoomAvatar(a) => StateContent::RoomAvatar {
            url: match a {
                StateEventContentChange::Original { content, .. } => content.url.clone(),
                StateEventContentChange::Redacted(_) => None,
            },
        },
        AnyOtherStateEventContentChange::RoomEncryption(e) => StateContent::RoomEncryption {
            algorithm: match e {
                StateEventContentChange::Original { content, .. } => content.algorithm.to_string(),
                StateEventContentChange::Redacted(_) => String::new(),
            },
        },
        _ => StateContent::OtherState {
            event_type: state.content().event_type().to_string(),
            state_key: state.state_key().to_string(),
        },
    }
}
