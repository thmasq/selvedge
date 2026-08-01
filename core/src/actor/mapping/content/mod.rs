pub mod msg_like;
pub mod state;
pub mod voip;

use matrix_sdk_ui::timeline::TimelineItemContent;
use ruma::UserId;
use selvedge_shared::model::TimelineContent;

pub fn resolve_content(
    content: &TimelineItemContent,
    own_user_id: Option<&UserId>,
) -> TimelineContent {
    match content {
        TimelineItemContent::MsgLike(msg) => msg_like::map(msg, own_user_id),

        TimelineItemContent::MembershipChange(m) => {
            TimelineContent::State(state::map_membership(m))
        }
        TimelineItemContent::ProfileChange(p) => TimelineContent::State(state::map_profile(p)),
        TimelineItemContent::OtherState(s) => TimelineContent::State(state::map_other(s)),

        TimelineItemContent::CallInvite => voip::map_invite(),
        TimelineItemContent::RtcNotification {
            call_intent,
            declined_by,
            ..
        } => voip::map_rtc(call_intent.as_ref(), declined_by),

        TimelineItemContent::FailedToParseMessageLike { event_type, error } => {
            TimelineContent::FailedToParseMessageLike {
                event_type: event_type.to_string(),
                error: error.to_string(),
            }
        }
        TimelineItemContent::FailedToParseState {
            event_type,
            state_key,
            error,
        } => TimelineContent::FailedToParseState {
            event_type: event_type.to_string(),
            state_key: state_key.clone(),
            error: error.to_string(),
        },
    }
}
