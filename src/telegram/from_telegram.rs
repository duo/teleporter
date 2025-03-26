use std::sync::Arc;

use anyhow::Result;
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use grammers_client::InputMessage;
use grammers_client::types::{Message, media};
use grammers_tl_types as tl;

use super::bridge::{Bridge, RemoteIdLock};
use super::{entities, telegram_helper as tg_helper};
use crate::common::{ChatType, Endpoint};
use crate::onebot::protocol::segment::Segment;
use crate::telegram::bridge;
use crate::{TelegramPylon, with_id_lock};

const GIF_THRESHOLD: usize = 100 * 1024;

impl TelegramPylon {
    pub async fn process_message(
        bridge: &Bridge,
        message: &Message,
        remote_id_lock: Arc<RemoteIdLock>,
    ) -> Result<()> {
        if !tg_helper::check_sender(bridge, message) {
            return Ok(());
        }

        // 忽略Action消息
        if message.action().is_some() {
            return Ok(());
        }

        let tg_chat_id = message.chat().id();
        match bridge.find_link_by_tg(tg_chat_id).await? {
            Some((_, remote_chat)) => {
                if let Some(remote_chat) = remote_chat {
                    with_id_lock!(remote_id_lock, remote_chat.to_id(), {
                        return Self::convert_and_send(bridge, &remote_chat, message).await;
                    });
                }
            }
            None => {
                if let Some(tl::enums::MessageReplyHeader::Header(header)) = message.reply_header()
                {
                    if header.forum_topic {
                        // 从Topic的ID查找对应的远端对话
                        if let Some(tg_topic_id) = header.reply_to_top_id.or(header.reply_to_msg_id)
                        {
                            if let Some(remote_chat) =
                                bridge.find_archive_by_tg(tg_chat_id, tg_topic_id).await?
                            {
                                with_id_lock!(remote_id_lock, remote_chat.to_id(), {
                                    return Self::convert_and_send(bridge, &remote_chat, message)
                                        .await;
                                });
                            }
                        }
                    } else if let Some(message_id) = header.reply_to_msg_id {
                        // 从回复的源消息查找对应的远端对话
                        if let Some((_, Some(remote_chat))) =
                            bridge.find_message_by_tg(tg_chat_id, message_id).await?
                        {
                            with_id_lock!(remote_id_lock, remote_chat.to_id(), {
                                return Self::convert_and_send(bridge, &remote_chat, message).await;
                            });
                        }
                    }
                }
            }
        }

        message
            .reply(InputMessage::html(
                "<b>The message can't be mapped to a remote chat</b>",
            ))
            .await?;

        Ok(())
    }

    async fn convert_and_send(
        bridge: &Bridge,
        remote_chat: &entities::remote_chat::Model,
        message: &Message,
    ) -> Result<()> {
        let (message_type, group_id, user_id) = match remote_chat.chat_type {
            ChatType::Private => (
                "private".to_string(),
                None,
                Some(remote_chat.target_id.clone()),
            ),
            ChatType::Group => (
                "group".to_string(),
                Some(remote_chat.target_id.clone()),
                None,
            ),
        };
        let mut segments: Vec<Segment> = Vec::new();

        if let Some(media) = message.media() {
            match &media {
                media::Media::Photo(_) => {
                    let (file_name, file_data) = bridge.download_media(&media).await?;
                    segments.push(Segment::Image(Segment::image(
                        Self::generate_file_base64(&file_data),
                        Some(file_name),
                        None,
                        None,
                        None,
                    )));
                }
                media::Media::Document(document) => {
                    let (mut file_name, file_data) = bridge.download_media(&media).await?;
                    if document.raw.voice {
                        // 语音
                        // TODO: Telegram的是oga后缀，改成ogg(微信可以播放ogg文件)
                        if let Some(fixed_name) = bridge::fix_filename(&file_name, "ogg") {
                            file_name = fixed_name;
                        }
                        segments.push(Segment::Record(Segment::record(
                            Self::generate_file_base64(&file_data),
                            Some(file_name),
                        )));
                    } else if document.raw.video {
                        // 视频
                        segments.push(Segment::Video(Segment::video(
                            Self::generate_file_base64(&file_data),
                            Some(file_name),
                            None,
                        )));
                    } else if tg_helper::is_raw_photo(document) {
                        // 未压缩图片
                        segments.push(Segment::Image(Segment::image(
                            Self::generate_file_base64(&file_data),
                            Some(file_name),
                            None,
                            None,
                            None,
                        )));
                    } else if tg_helper::is_gif(document) {
                        // GIF表情 (Telegram里使用MP4格式保存的)
                        // TODO: 大于阈值的以视频发送, 小于的转成GIF(微信发送大的GIF非常慢)
                        if file_data.len() > GIF_THRESHOLD {
                            segments.push(Segment::Video(Segment::video(
                                Self::generate_file_base64(&file_data),
                                Some(file_name),
                                None,
                            )));
                        } else {
                            match tg_helper::video_to_gif(&file_data).await {
                                Ok(gif_data) => {
                                    if let Some(fixed_name) =
                                        bridge::fix_filename(&file_name, "gif")
                                    {
                                        file_name = fixed_name;
                                    }
                                    segments.push(Segment::Image(Segment::image(
                                        Self::generate_file_base64(&gif_data),
                                        Some(file_name),
                                        None,
                                        None,
                                        None,
                                    )));
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to convert video to gif: {}", e);
                                }
                            }
                        }
                    } else {
                        // 文件
                        segments.push(Segment::File(Segment::file(
                            Self::generate_file_base64(&file_data),
                            Some(file_name),
                        )));
                    }
                }
                media::Media::Sticker(sticker) => {
                    let (mut file_name, file_data) = bridge.download_media(&media).await?;
                    match sticker.document.mime_type() {
                        Some("video/webm") => match tg_helper::webm_to_gif(&file_data).await {
                            Ok(gif_data) => {
                                if let Some(fixed_name) = bridge::fix_filename(&file_name, "gif") {
                                    file_name = fixed_name;
                                }
                                segments.push(Segment::Image(Segment::image(
                                    Self::generate_file_base64(&gif_data),
                                    Some(file_name),
                                    None,
                                    None,
                                    None,
                                )));
                            }
                            Err(e) => {
                                tracing::warn!("Failed to convert webm to gif: {}", e);
                            }
                        },
                        Some("application/x-tgsticker") => {
                            match tg_helper::tgs_to_gif(sticker.document.id(), &file_data).await {
                                Ok(gif_data) => {
                                    if let Some(fixed_name) =
                                        bridge::fix_filename(&file_name, "gif")
                                    {
                                        file_name = fixed_name;
                                    }
                                    segments.push(Segment::Image(Segment::image(
                                        Self::generate_file_base64(&gif_data),
                                        Some(file_name),
                                        None,
                                        None,
                                        None,
                                    )));
                                }
                                Err(e) => {
                                    tracing::warn!("Failed to convert tgs to gif: {}", e);
                                }
                            }
                        }
                        Some(_) => {
                            // TODO: 不支持的先当文件发送了
                            segments.push(Segment::File(Segment::file(
                                Self::generate_file_base64(&file_data),
                                Some(file_name),
                            )));
                        }
                        None => {}
                    }
                }
                media::Media::Geo(geo) => {
                    let content = format!(
                        "Latitude: {:.5} Longitude: {:.5}",
                        geo.latitue(),
                        geo.longitude()
                    );
                    if let Ok(segment) = Self::generate_location_segment(
                        &remote_chat.endpoint,
                        "Location",
                        &content,
                        geo.latitue(),
                        geo.longitude(),
                    ) {
                        segments.push(segment);
                    }
                }
                media::Media::Venue(venue) => {
                    if let Some(geo) = tg_helper::get_geo(venue) {
                        if let Ok(segment) = Self::generate_location_segment(
                            &remote_chat.endpoint,
                            venue.title(),
                            venue.address(),
                            geo.0,
                            geo.1,
                        ) {
                            segments.push(segment);
                        }
                    }
                }
                _ => {
                    // TODO: add more media support
                }
            }
        }

        if !message.text().is_empty() {
            segments.push(Segment::Text(Segment::text(message.text().to_string())));
        }

        if !segments.is_empty() {
            // 检查是否有回复的消息
            let reply_to_msg_id = match message.reply_header() {
                Some(tl::enums::MessageReplyHeader::Header(header)) => {
                    if header.forum_topic {
                        // 如果是Topic消息, 那么reply_to_top_id和reply_to_msg_id同时有值才是回复
                        if header.reply_to_top_id.is_some() {
                            header.reply_to_msg_id
                        } else {
                            None
                        }
                    } else {
                        header.reply_to_msg_id
                    }
                }
                _ => None,
            };
            if let Some(message_id) = reply_to_msg_id {
                if let Some((message, _)) = bridge
                    .find_message_by_tg(message.chat().id(), message_id)
                    .await?
                {
                    // QQ如果Reply不是第一个消息段的话, 会往消息末尾添加@
                    segments.insert(0, Segment::Reply(Segment::reply(message.remote_msg_id)));
                }
            }

            match bridge
                .send_msg(
                    &remote_chat.endpoint,
                    message_type,
                    group_id,
                    user_id,
                    segments,
                )
                .await
            {
                Ok(message_id) => {
                    bridge
                        .save_message_by_remote(remote_chat.id, &message_id.message_id, message)
                        .await?;
                }
                Err(e) => {
                    tracing::warn!("Failed to send message to remote: {}", e);
                    message
                        .reply(InputMessage::html(
                            "<b>Failed to send message to remote</b>",
                        ))
                        .await?;
                }
            }
        } else {
            message
                .reply(InputMessage::html(
                    "<b>Failed to convert message for remote</b>",
                ))
                .await?;
        }

        Ok(())
    }

    fn generate_file_base64(data: &[u8]) -> String {
        format!("base64://{}", BASE64_STANDARD.encode(data))
    }

    fn generate_location_segment(
        endpoint: &Endpoint,
        title: &str,
        content: &str,
        lat: f64,
        lon: f64,
    ) -> Result<Segment> {
        match endpoint.platform {
            crate::common::Platform::QQ => {
                let location_json = format!(
                    r#"
                    {{
                        "app": "com.tencent.map",
                        "desc": "地图",
                        "view": "LocationShare",
                        "ver": "0.0.0.1",
                        "prompt": "[位置]{}",
                        "from": 1,
                        "meta": {{
                            "Location.Search": {{
                                "id": "12250896297164027526",
                                "name": "{}",
                                "address": "{}",
                                "lat": "{:.5}",
                                "lng": "{:.5}",
                                "from": "plusPanel"
                            }}
                        }},
                        "config": {{
                            "forward": 1,
                            "autosize": 1,
                            "type": "card"
                        }}
                    }}
                    "#,
                    title, title, content, lat, lon
                );
                Ok(Segment::Json(Segment::json(location_json)))
            }
            crate::common::Platform::WeChat => Ok(Segment::Location(Segment::location(
                lat,
                lon,
                Some(title.to_owned()),
                Some(content.to_owned()),
            ))),
            _ => Err(anyhow::anyhow!("invalid endpoint")),
        }
    }
}
