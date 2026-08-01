use matrix_sdk::ruma::events::rtc::notification::CallIntent;
use ruma::OwnedUserId;
use selvedge_shared::model::TimelineContent;

pub const fn map_invite() -> TimelineContent {
    TimelineContent::CallInvite
}

pub fn map_rtc(intent: Option<&CallIntent>, declined_by: &[OwnedUserId]) -> TimelineContent {
    TimelineContent::RtcNotification {
        call_intent: intent.as_ref().map(std::string::ToString::to_string),
        declined_by: declined_by.to_vec(),
    }
}
