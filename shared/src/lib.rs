pub mod event;
pub mod macros;
pub mod message;
pub mod model;

use ammonia::Builder;
pub use model::{
    ActiveCallState, ActorError, AudioInfoWrapper, CallContent, CallParticipant, CallType,
    DeliveryStatus, DeviceInfo, EncryptionStatus, EventItem, ImageInfo, LinkPreview, MediaSource,
    MemberProfile, MessageContent, ModelError, PollAnswer, PollState, ReactionDetails,
    ReplyDetails, RoomDetails, RoomListEntryDiff, RoomListEntryView, RoomPermissions, RoomSummary,
    RoomTrustLevel, StateContent, ThumbnailInfo, TimelineContent, TimelineDiff, TimelineItem,
    VerificationRequest, VerificationState, VideoInfo, VirtualItem,
};

#[must_use]
pub fn sanitize_matrix_html(raw_html: &str) -> String {
    Builder::default()
        .add_tags([
            "span", "del", "u", "table", "thead", "tbody", "tr", "th", "td", "details", "summary",
        ])
        .add_tag_attributes("code", ["class"])
        .add_tag_attributes(
            "span",
            [
                "data-mx-spoiler",
                "data-mx-maths",
                "data-mx-bg-color",
                "data-mx-color",
            ],
        )
        .add_tag_attributes("div", ["data-mx-maths"])
        .add_url_schemes(["http", "https", "ftp", "mailto", "magnet", "matrix", "mxc"])
        .clean(raw_html)
        .to_string()
}
