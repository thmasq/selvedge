use matrix_sdk::ruma::events::StateEventContentChange;
use matrix_sdk_ui::timeline::{
    AnyOtherStateEventContentChange, MemberProfileChange, MembershipChange, OtherState,
    RoomMembershipChange,
};

pub fn map_membership(change: &RoomMembershipChange) -> String {
    let user_id = change.user_id();

    match change.change() {
        Some(MembershipChange::Joined) => format!("{user_id} joined the room"),
        Some(MembershipChange::Left) => format!("{user_id} left the room"),
        Some(MembershipChange::Banned) => format!("{user_id} was banned"),
        Some(MembershipChange::Unbanned) => format!("{user_id} was unbanned"),
        Some(MembershipChange::Kicked) => format!("{user_id} was kicked"),
        Some(MembershipChange::KickedAndBanned) => format!("{user_id} was kicked and banned"),
        Some(MembershipChange::Invited) => format!("{user_id} was invited"),
        Some(MembershipChange::InvitationAccepted) => format!("{user_id} accepted the invitation"),
        Some(MembershipChange::InvitationRejected) => format!("{user_id} rejected the invitation"),
        Some(MembershipChange::InvitationRevoked) => format!("{user_id}'s invitation was revoked"),
        Some(MembershipChange::Knocked) => format!("{user_id} requested to join"),
        Some(MembershipChange::KnockAccepted) => {
            format!("{user_id}'s request to join was accepted")
        }
        Some(MembershipChange::KnockRetracted) => {
            format!("{user_id} retracted their request to join")
        }
        Some(MembershipChange::KnockDenied) => format!("{user_id}'s request to join was denied"),
        Some(MembershipChange::Error) => format!("{user_id} encountered a membership error"),
        Some(MembershipChange::NotImplemented) | None => format!("{user_id}'s membership changed"),
        _ => format!("{user_id}'s membership changed"),
    }
}

pub fn map_profile(change: &MemberProfileChange) -> String {
    let user_id = change.user_id();

    if change.displayname_change().is_some() && change.avatar_url_change().is_some() {
        format!("{user_id} changed their display name and avatar")
    } else if let Some(dn) = change.displayname_change() {
        let old = dn.old.as_deref().unwrap_or("Unknown");
        let new = dn.new.as_deref().unwrap_or("Unknown");
        format!("{user_id} changed their display name from '{old}' to '{new}'")
    } else if change.avatar_url_change().is_some() {
        format!("{user_id} changed their avatar")
    } else {
        format!("{user_id} updated their profile")
    }
}

pub fn map_other(state: &OtherState) -> String {
    match state.content() {
        AnyOtherStateEventContentChange::RoomName(n) => {
            let name = match n {
                StateEventContentChange::Original { content, .. } => content.name.clone(),
                StateEventContentChange::Redacted(_) => "(redacted)".to_string(),
            };
            format!("The room name was changed to '{name}'")
        }
        AnyOtherStateEventContentChange::RoomTopic(t) => {
            let topic = match t {
                StateEventContentChange::Original { content, .. } => content.topic.clone(),
                StateEventContentChange::Redacted(_) => "(redacted)".to_string(),
            };
            format!("The room topic was changed to '{topic}'")
        }
        AnyOtherStateEventContentChange::RoomAvatar(_) => "The room avatar was changed".to_string(),
        AnyOtherStateEventContentChange::RoomEncryption(e) => {
            let algo = match e {
                StateEventContentChange::Original { content, .. } => content.algorithm.to_string(),
                StateEventContentChange::Redacted(_) => "unknown".to_string(),
            };
            format!("Encryption was enabled ({algo})")
        }
        _ => {
            let event_type = state.content().event_type();
            format!("Room state updated ({event_type})")
        }
    }
}
