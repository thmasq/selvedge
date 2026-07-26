use crate::actor::MatrixActor;
use matrix_sdk::attachment::BaseAudioInfo;
use matrix_sdk::attachment::BaseFileInfo;
use matrix_sdk::attachment::BaseImageInfo;
use matrix_sdk::attachment::BaseVideoInfo;
use matrix_sdk::attachment::{AttachmentConfig, AttachmentInfo, Thumbnail};
use matrix_sdk::ruma::api::client::authenticated_media::get_media_config::v1::Request as AuthMediaRequest;
#[allow(deprecated)]
use matrix_sdk::ruma::api::client::media::get_media_config::v3::Request as LegacyMediaRequest;
use matrix_sdk::ruma::events::room::message::TextMessageEventContent;
use selvedge_shared::event::ToShell;
use selvedge_shared::event::core::CoreEvents;
use selvedge_shared::event::core::command_result::CommandResultArgs;
use selvedge_shared::message::messaging::send_media::SendMediaArgs;
use selvedge_shared::model::ActorError;
use std::cell::RefCell;
use std::str::FromStr;

thread_local! {
    static UPLOAD_LIMIT: RefCell<Option<u64>> = RefCell::new(None);
}

pub async fn run(actor: &MatrixActor, args: SendMediaArgs) -> Vec<ToShell> {
    let client = actor.client.borrow().clone();

    if let Some(client) = client {
        let limit = UPLOAD_LIMIT.with(|l| *l.borrow());
        let max_size = if let Some(size) = limit {
            size
        } else {
            let req = AuthMediaRequest::new();
            if let Ok(res) = client.send(req).await {
                let size = u64::from(res.upload_size);
                UPLOAD_LIMIT.with(|l| *l.borrow_mut() = Some(size));
                size
            } else {
                // Fallback for older homeservers
                #[allow(deprecated)]
                let legacy_req = LegacyMediaRequest::new();
                if let Ok(res) = client.send(legacy_req).await {
                    let size = u64::from(res.upload_size);
                    UPLOAD_LIMIT.with(|l| *l.borrow_mut() = Some(size));
                    size
                } else {
                    u64::MAX
                }
            }
        };

        if args.data.len() as u64 > max_size {
            return vec![ToShell::Core(CoreEvents::CommandResult(
                CommandResultArgs {
                    request_id: args.request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed(format!(
                        "File size {} bytes exceeds server limit of {} bytes",
                        args.data.len(),
                        max_size
                    ))),
                },
            ))];
        }

        if let Some(room) = client.get_room(&args.room_id) {
            let mut config = AttachmentConfig::new();

            if let Some(caption) = args.caption {
                config = config.caption(Some(TextMessageEventContent::plain(caption)));
            }

            if let Ok(mime) = mime::Mime::from_str(&args.mime_type) {
                let width = args.width.map(Into::into);
                let height = args.height.map(Into::into);
                let size = Some((args.data.len() as u32).into());

                let info = match mime.type_() {
                    mime::IMAGE => {
                        let mut info = BaseImageInfo::default();
                        info.width = width;
                        info.height = height;
                        info.size = size;
                        info.blurhash = args.blurhash;
                        AttachmentInfo::Image(info)
                    }
                    mime::VIDEO => {
                        let mut info = BaseVideoInfo::default();
                        info.width = width;
                        info.height = height;
                        info.size = size;
                        info.blurhash = args.blurhash;
                        AttachmentInfo::Video(info)
                    }
                    mime::AUDIO => {
                        let mut info = BaseAudioInfo::default();
                        info.size = size;
                        AttachmentInfo::Audio(info)
                    }
                    _ => {
                        let mut info = BaseFileInfo::default();
                        info.size = size;
                        AttachmentInfo::File(info)
                    }
                };

                config = config.info(info);

                if let (Some(t_data), Some(t_mime), Some(w), Some(h)) = (
                    args.thumbnail_data,
                    args.thumbnail_mime,
                    args.width,
                    args.height,
                ) {
                    if let Ok(thumb_mime) = mime::Mime::from_str(&t_mime) {
                        let size = (t_data.len() as u32).into();

                        let thumbnail = Thumbnail {
                            data: t_data,
                            content_type: thumb_mime,
                            width: w.into(),
                            height: h.into(),
                            size,
                        };
                        config = config.thumbnail(Some(thumbnail));
                    }
                }

                match room
                    .send_attachment(&args.filename, &mime, args.data, config)
                    .await
                {
                    Ok(_) => vec![ToShell::Core(CoreEvents::CommandResult(
                        CommandResultArgs {
                            request_id: args.request_id,
                            success: true,
                            error: None,
                        },
                    ))],
                    Err(e) => vec![ToShell::Core(CoreEvents::CommandResult(
                        CommandResultArgs {
                            request_id: args.request_id,
                            success: false,
                            error: Some(ActorError::RoomOperationFailed(e.to_string())),
                        },
                    ))],
                }
            } else {
                vec![ToShell::Core(CoreEvents::CommandResult(
                    CommandResultArgs {
                        request_id: args.request_id,
                        success: false,
                        error: Some(ActorError::RoomOperationFailed(
                            "Invalid MIME type".to_string(),
                        )),
                    },
                ))]
            }
        } else {
            vec![ToShell::Core(CoreEvents::CommandResult(
                CommandResultArgs {
                    request_id: args.request_id,
                    success: false,
                    error: Some(ActorError::RoomOperationFailed("Room not found".into())),
                },
            ))]
        }
    } else {
        vec![ToShell::Core(CoreEvents::CommandResult(
            CommandResultArgs {
                request_id: args.request_id,
                success: false,
                error: Some(ActorError::ClientNotInitialized),
            },
        ))]
    }
}
