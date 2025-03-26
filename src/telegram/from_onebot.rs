use std::fmt::Write;
use std::sync::Arc;

use anyhow::Result;
use grammers_client::session::PackedType;
use grammers_client::types::{Chat, InputMedia};
use grammers_client::{InputMessage, button, reply_markup};
use grammers_tl_types::enums::{InputGeoPoint, InputStickerSet};
use grammers_tl_types::types::{
    DocumentAttributeFilename, DocumentAttributeSticker, InputMediaUploadedDocument,
    InputMediaVenue,
};
use sea_orm::ActiveValue::Set;
use sea_orm::{ActiveModelTrait, IntoActiveModel};
use serde_json::Value;
use uuid::Uuid;

use super::bridge::RelayBridge;
use super::{entities, onebot_helper as ob_helper};
use crate::TelegramPylon;
use crate::common::{ChatType, DeliveryStatus, Endpoint, Platform};
use crate::onebot::protocol::OnebotEvent;
use crate::onebot::protocol::event::{Event, MessageEvent, MetaEvent, NoticeEvent};
use crate::onebot::protocol::segment::Segment;

const BIG_FILE_SIZE: usize = 10 * 1024 * 1024;
const IMAGE_SLIDE_LIMIT: u32 = 2560;

enum TgMsgType {
    Text,
    Html,
    Photo,
    Sticker,
    Voice,
    Video,
    Document,
    Location,
}

impl TelegramPylon {
    pub async fn handle_event(bridge: &RelayBridge, event: OnebotEvent) -> Result<()> {
        match &*event.raw {
            Event::Message(message) => {
                Self::process_onebot_message(bridge, &event.endpoint, message).await?
            }
            Event::MessageSent(message) => {
                Self::process_onebot_message(bridge, &event.endpoint, message).await?
            }
            Event::Meta(meta) => Self::process_onebot_meta(bridge, &event.endpoint, meta).await?,
            Event::Notice(notice) => {
                Self::process_onebot_notice(bridge, &event.endpoint, notice).await?
            }
            _ => {}
        }

        Ok(())
    }

    async fn process_onebot_message(
        bridge: &RelayBridge,
        endpoint: &Endpoint,
        message: &MessageEvent,
    ) -> Result<()> {
        tracing::info!("Received Onebot message: {:?}", message);

        // 跳过空消息
        if message.message.is_empty() {
            return Ok(());
        }

        let remote_chat = bridge
            .get_remote_chat(endpoint, &message.get_chat_type(), &message.get_chat_id())
            .await?;

        // 检查消息是否处理过
        if (bridge
            .find_message_by_remote(remote_chat.id, &message.message_id)
            .await?)
            .is_some()
        {
            tracing::info!("Ignoring duplicated message: {:?}", message);
            return Ok(());
        }

        let (chat, mut reply_to, mut title) = Self::fetch_chat_and_title(
            bridge,
            endpoint,
            remote_chat.clone(),
            &message.sender.display_name(),
        )
        .await?;

        // 遍历消息里的各片段进行转换处理
        let mut msg_type = TgMsgType::Text;
        let mut content = String::new();
        let mut media_uploaded = Vec::new();
        let mut location = None;
        for segment in &(message.message) {
            match segment {
                Segment::Text(seg) => match endpoint.platform {
                    Platform::WeChat => {
                        content.push_str(&ob_helper::replace_wechat_emoji(&seg.text));
                    }
                    _ => {
                        content.push_str(&seg.text);
                    }
                },
                Segment::Face(seg) => match endpoint.platform {
                    Platform::QQ => {
                        content.push_str(ob_helper::replace_qq_face(&seg.id).as_str());
                    }
                    _ => {
                        write!(&mut content, "/[Face{}]", seg.id).unwrap();
                    }
                },
                Segment::At(seg) => {
                    match bridge
                        .get_group_member_info(
                            endpoint,
                            message.group_id.as_ref().unwrap().clone(),
                            seg.id.clone(),
                            true,
                        )
                        .await
                    {
                        Ok(member) => {
                            write!(&mut content, "@{}", member.display_name()).unwrap();
                        }
                        Err(_) => {
                            write!(&mut content, "@{}", seg.id).unwrap();
                        }
                    }
                }
                Segment::Image(_) => match bridge.upload_segment(endpoint, segment).await {
                    Ok(uploaded) => {
                        media_uploaded.push(uploaded);
                        content.push_str("[图片]");
                        if ob_helper::is_sticker(segment) {
                            msg_type = TgMsgType::Sticker;
                        } else {
                            msg_type = TgMsgType::Photo;
                        }
                    }
                    Err(e) => {
                        content.push_str("[图片上传失败]");
                        tracing::warn!("Failed to upload photo: {}", e)
                    }
                },
                Segment::MarketFace(_) => match bridge.upload_segment(endpoint, segment).await {
                    Ok(uploaded) => {
                        media_uploaded.push(uploaded);
                        content.push_str("[表情]");
                        msg_type = TgMsgType::Sticker;
                    }
                    Err(e) => {
                        content.push_str("[表情上传失败]");
                        tracing::warn!("Failed to upload sticker: {}", e)
                    }
                },
                Segment::Record(_) => match bridge.upload_segment(endpoint, segment).await {
                    Ok(uploaded) => {
                        media_uploaded.push(uploaded);
                        content.push_str("[语音]");
                        msg_type = TgMsgType::Voice;
                    }
                    Err(e) => {
                        content.push_str("[语音上传失败]");
                        tracing::warn!("Failed to upload record: {}", e)
                    }
                },
                Segment::Video(_) => match bridge.upload_segment(endpoint, segment).await {
                    Ok(uploaded) => {
                        media_uploaded.push(uploaded);
                        content.push_str("[视频]");
                        msg_type = TgMsgType::Video;
                    }
                    Err(e) => {
                        content.push_str("[视频上传失败]");
                        tracing::warn!("Failed to upload video: {}", e)
                    }
                },
                Segment::File(_) => match bridge.upload_segment(endpoint, segment).await {
                    Ok(uploaded) => {
                        media_uploaded.push(uploaded);
                        content.push_str("[文件]");
                        msg_type = TgMsgType::Document;
                    }
                    Err(e) => {
                        content.push_str("[文件上传失败]");
                        tracing::warn!("Failed to upload file: {}", e)
                    }
                },
                Segment::Reply(seg) => {
                    if let Some(entity) = bridge
                        .find_message_by_remote(remote_chat.id, &seg.id)
                        .await?
                    {
                        reply_to = Some(entity.tg_msg_id);
                    }
                }
                Segment::Forward(_) => {
                    content.push_str("[合并消息]");
                }
                Segment::Location(seg) => {
                    location = Some(InputMediaVenue {
                        geo_point: InputGeoPoint::Point(grammers_tl_types::types::InputGeoPoint {
                            lat: seg.lat,
                            long: seg.lon,
                            accuracy_radius: None,
                        }),
                        title: seg.title.as_deref().unwrap_or("").to_string(),
                        address: seg.content.as_deref().unwrap_or("").to_string(),
                        provider: String::new(),
                        venue_id: String::new(),
                        venue_type: String::new(),
                    });
                    msg_type = TgMsgType::Location;
                }
                Segment::Share(seg) => {
                    write!(
                        &mut content,
                        "<u>{}</u>\n\n{}\n\nvia <a href=\"{}\">{}</a>",
                        html_escape::encode_text(&seg.title),
                        html_escape::encode_text(seg.content.as_deref().unwrap_or("")),
                        html_escape::encode_text(&seg.url),
                        html_escape::encode_text(&seg.title),
                    )
                    .unwrap();
                    msg_type = TgMsgType::Html;
                }
                Segment::Json(seg) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&seg.data) {
                        let view = v.get("view").and_then(Value::as_str).unwrap_or("");
                        if view == "LocationShare" {
                            location = Some(ob_helper::extract_location_from_json(&v)?);
                            msg_type = TgMsgType::Location;
                            break;
                        } else {
                            let share = ob_helper::extract_share_from_json(&v)?;
                            if !share.is_empty() {
                                content.push_str(&share);
                                msg_type = TgMsgType::Html;
                                break;
                            }
                        }
                    }

                    content.push_str(&seg.data);
                }
                _ => {}
            }
        }

        // 发送转换后的消息到Telegram
        let ret;
        match msg_type {
            TgMsgType::Text => {
                write!(&mut title, "\n{}", &content).unwrap();
                let message = InputMessage::text(title).reply_to(reply_to);
                ret = vec![Some(bridge.send_telegram_message(&*chat, message).await?)];
            }
            TgMsgType::Html => {
                write!(&mut title, "\n{}", &content).unwrap();
                let message = InputMessage::html(title)
                    .reply_to(reply_to)
                    .link_preview(true);
                ret = vec![Some(bridge.send_telegram_message(&*chat, message).await?)];
            }
            TgMsgType::Photo => {
                if media_uploaded.len() == 1 {
                    // 也是图文混合
                    if message.message.len() > 1 {
                        write!(&mut title, "\n{}", &content).unwrap();
                    }
                    // TODO: 判断图片大小和尺寸决定发送图片还是文件
                    let media = media_uploaded.pop().unwrap();
                    let mut message = InputMessage::text(&title).reply_to(reply_to);
                    if media.file_size > BIG_FILE_SIZE
                        || media.width > IMAGE_SLIDE_LIMIT
                        || media.height > IMAGE_SLIDE_LIMIT
                    {
                        message = message.document(media.uploaded);
                    } else {
                        message = message.photo(media.uploaded);
                        /*
                        match bridge.bot_client.send_message(&*chat, message).await {
                            Ok(message) => ret = vec![Some(message)],
                            Err(_) => {
                                // 失败则发送原图
                                let message = InputMessage::text(&title)
                                    .document(media.uploaded)
                                    .reply_to(reply_to);
                                ret = vec![
                                    bridge.bot_client.send_message(&*chat, message).await.ok(),
                                ];
                            }
                        }
                        */
                    }
                    ret = vec![Some(bridge.send_telegram_message(&*chat, message).await?)];
                } else {
                    write!(&mut title, "\n{}", &content).unwrap();
                    ret = bridge
                        .send_telegram_album(
                            &*chat,
                            media_uploaded
                                .iter()
                                .map(|u| {
                                    InputMedia::caption(&title)
                                        .photo(u.uploaded.clone())
                                        .reply_to(reply_to)
                                })
                                .collect(),
                        )
                        .await?;
                }
            }
            TgMsgType::Sticker => {
                let upload_info = media_uploaded.pop().unwrap();

                // TODO: QQ里魔法表情可以和文字混合, 目前这逻辑会忽略掉文字内容了...
                let message = InputMessage::text(&title)
                    .media(InputMediaUploadedDocument {
                        nosound_video: false,
                        force_file: false,
                        spoiler: false,
                        file: upload_info.uploaded.raw,
                        thumb: None,
                        mime_type: upload_info.mime_type,
                        attributes: vec![
                            (DocumentAttributeFilename {
                                file_name: upload_info.file_name,
                            })
                            .into(),
                            (DocumentAttributeSticker {
                                mask: false,
                                alt: "😊".to_string(),
                                stickerset: InputStickerSet::Empty,
                                mask_coords: None,
                            })
                            .into(),
                        ],
                        stickers: None,
                        ttl_seconds: None,
                        video_cover: None,
                        video_timestamp: None,
                    })
                    .reply_markup(&reply_markup::inline(vec![vec![button::url(
                        &title,
                        "tg://sticker",
                    )]]))
                    .reply_to(reply_to);

                ret = vec![Some(bridge.send_telegram_message(&*chat, message).await?)];
            }
            TgMsgType::Voice => {
                let message = InputMessage::text(title)
                    .document(media_uploaded.pop().unwrap().uploaded)
                    .reply_to(reply_to);
                // TODO: 增加语音持续时间
                ret = vec![Some(bridge.send_telegram_message(&*chat, message).await?)];
            }
            TgMsgType::Video => {
                let message = InputMessage::text(title)
                    .document(media_uploaded.pop().unwrap().uploaded)
                    .reply_to(reply_to);
                ret = vec![Some(bridge.send_telegram_message(&*chat, message).await?)];
            }
            TgMsgType::Document => {
                let message = InputMessage::text(title)
                    .file(media_uploaded.pop().unwrap().uploaded)
                    .reply_to(reply_to);
                ret = vec![Some(bridge.send_telegram_message(&*chat, message).await?)];
            }
            TgMsgType::Location => {
                let message = InputMessage::text(&title)
                    .media(location.unwrap())
                    .reply_to(reply_to);
                ret = vec![Some(bridge.send_telegram_message(&*chat, message).await?)];
            }
        }

        tracing::debug!("Send to telegram return: {:?}", ret);

        // 保存消息映射关系以及建立消息索引
        for msg in ret.iter().flatten() {
            if let Err(e) = bridge.index_message(msg).await {
                tracing::warn!("Failed to index message: {}", e);
            }

            if let Err(e) = bridge
                .save_message_by_remote(remote_chat.id, &message.message_id, msg)
                .await
            {
                tracing::warn!("Failed to insert message mapping: {}", e);
            }
        }

        Ok(())
    }

    async fn process_onebot_meta(
        bridge: &RelayBridge,
        endpoint: &Endpoint,
        meta: &MetaEvent,
    ) -> Result<()> {
        tracing::debug!("Received meta: {:?}", meta);
        if let MetaEvent::Lifecycle(meta) = meta {
            match meta.sub_type.as_str() {
                "connect" => {
                    // 更新好友的信息
                    let friend_list = bridge.get_friend_list(endpoint).await?;
                    for info in friend_list.as_ref() {
                        if let Err(e) = bridge.update_remote_private_chat(endpoint, info).await {
                            tracing::warn!("Failed to update remote private chat: {}", e)
                        }
                    }
                    // 更新群组的信息
                    let group_list = bridge.get_group_list(endpoint).await?;
                    for info in group_list.as_ref() {
                        if let Err(e) = bridge.update_remote_group_chat(endpoint, info).await {
                            tracing::warn!("Failed to update remote group chat: {}", e)
                        }
                    }

                    // 提示远端连接
                    let chat = bridge
                        .get_tg_chat(PackedType::User, bridge.admin_id)
                        .await?;
                    let message =
                        InputMessage::html(format!("<b>[INFO] {} connected</b>", endpoint));
                    bridge.send_telegram_message(&*chat, message).await?;
                }
                "disconnect" => {
                    // 提示远程断开
                    let chat = bridge
                        .get_tg_chat(PackedType::User, bridge.admin_id)
                        .await?;
                    let message =
                        InputMessage::html(format!("<b>[INFO] {} disconnected</b>", endpoint));
                    bridge.send_telegram_message(&*chat, message).await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn process_onebot_notice(
        bridge: &RelayBridge,
        endpoint: &Endpoint,
        notice: &NoticeEvent,
    ) -> Result<()> {
        tracing::debug!("Received notice: {:?}", notice);
        let (message_id, sender_name, remote_chat) = match notice {
            NoticeEvent::FriendRecall(event) => {
                // FIXME: 在私聊里自己撤回的没有对方的标识
                if event.self_id == event.user_id {
                    return Ok(());
                }
                (
                    &event.message_id,
                    &bridge
                        .get_stranger_info(endpoint, event.user_id.clone(), false)
                        .await?
                        .display_name(),
                    bridge
                        .get_remote_chat(endpoint, &ChatType::Private, &event.user_id)
                        .await?,
                )
            }
            NoticeEvent::GroupRecall(event) => (
                &event.message_id,
                &bridge
                    .get_group_member_info(
                        endpoint,
                        event.group_id.clone(),
                        event.user_id.clone(),
                        false,
                    )
                    .await?
                    .display_name(),
                bridge
                    .get_remote_chat(endpoint, &ChatType::Group, &event.group_id)
                    .await?,
            ),
            _ => return Ok(()),
        };

        if let Some(msg) = bridge
            .find_message_by_remote(remote_chat.id, message_id)
            .await?
        {
            let tg_msg_id = msg.tg_msg_id;

            // 更新原始消息为已撤回
            let mut active_model = msg.into_active_model();
            active_model.delivery_status = Set(DeliveryStatus::Recalled);
            active_model.update(&bridge.db).await?;

            let (tg_chat, _, mut title) =
                Self::fetch_chat_and_title(bridge, endpoint, remote_chat.clone(), sender_name)
                    .await?;

            title.push_str("\n<del>Recalled this message</del>");
            let message = InputMessage::html(title).reply_to(Some(tg_msg_id));

            // 保存消息映射关系
            let msg = bridge
                .bot_client
                .send_message(tg_chat.as_ref(), message)
                .await?;
            let fake_id = format!("fake:{}", Uuid::new_v4().simple());
            bridge
                .save_message_by_remote(remote_chat.id, &fake_id, &msg)
                .await?;
        }

        Ok(())
    }

    // 获取Telegram消息的目标对话以及标题
    async fn fetch_chat_and_title(
        bridge: &RelayBridge,
        endpoint: &Endpoint,
        remote_chat: Arc<entities::remote_chat::Model>,
        sender_name: &str,
    ) -> Result<(Arc<Chat>, Option<i32>, String)> {
        let target = bridge
            .get_remote_chat(endpoint, &remote_chat.chat_type, &remote_chat.target_id)
            .await?;

        // 查找链接群
        match bridge.find_link_by_remote(remote_chat.id).await? {
            Some(link) => {
                let packed_type = match link.tg_chat_type {
                    0b0000_0010 => PackedType::User,
                    0b0000_0011 => PackedType::Bot,
                    0b0000_0100 => PackedType::Chat,
                    0b0010_1000 => PackedType::Megagroup,
                    0b0011_0000 => PackedType::Broadcast,
                    0b0011_1000 => PackedType::Gigagroup,
                    _ => PackedType::User,
                };
                Ok((
                    bridge.get_tg_chat(packed_type, link.tg_chat_id).await?,
                    None,
                    format!("{}:", sender_name),
                ))
            }
            None => match bridge.find_archive_by_endpoint(endpoint).await? {
                // 查找归档群
                Some(archive) => {
                    let tg_topic_id = bridge.get_or_create_topic(&archive, &remote_chat).await?;
                    Ok((
                        bridge
                            .get_tg_chat(PackedType::Megagroup, archive.tg_chat_id)
                            .await?,
                        Some(tg_topic_id),
                        format!("{}:", sender_name),
                    ))
                }
                // 没有归档群则发送给管理员
                None => Ok((
                    bridge
                        .get_tg_chat(PackedType::User, bridge.admin_id)
                        .await?,
                    None,
                    match &remote_chat.chat_type {
                        ChatType::Private => format!("👤 {}:", target.name),
                        ChatType::Group => format!("👥 {} [{}]:", sender_name, target.name),
                    },
                )),
            },
        }
    }
}

impl MessageEvent {
    pub fn get_chat_type(&self) -> ChatType {
        match self.message_type.as_str() {
            "group" => ChatType::Group,
            _ => ChatType::Private,
        }
    }
}
