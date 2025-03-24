use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::num::NonZeroU32;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use chrono::Utc;
use dashmap::DashMap;
use governor::{Quota, RateLimiter};
use grammers_client::Client;
use grammers_client::session::PackedType;
use grammers_client::types::media::{Document, Uploaded};
use grammers_client::types::{Chat, Message, PackedChat};
use grammers_tl_types as tl;
use regex::Regex;
use reqwest::Url;
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use sea_orm::ActiveValue::Set;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, sea_query,
};
use tokio::sync::{Mutex, mpsc};

use super::{entities, onebot_helper as ob_helper};
use crate::common::{ChatType, DeliveryStatus, Endpoint, Platform, RemoteChatKey};
use crate::onebot::onebot_pylon::OnebotPylon;
use crate::onebot::protocol::OnebotRequest;
use crate::onebot::protocol::request::{
    DeleteMsg, GetFile, GetGroupInfo, GetGroupMemberInfo, GetGroupMemberList, GetImage, GetRecord,
    GetStrangerInfo, Request, SendMsg,
};
use crate::onebot::protocol::response::{
    FileInfo, GroupInfo, MemberInfo, MessageId, ResponseData, UserInfo,
};
use crate::onebot::protocol::segment::Segment;

pub type RelayBridge = Arc<Bridge>;
pub type ChatModel = entities::remote_chat::Model;

pub type RemoteIdLock = DashMap<RemoteChatKey, Arc<Mutex<()>>>;
pub type TgIdLock = DashMap<i64, Arc<Mutex<()>>>;

type GovernorStateMap = DashMap<i64, governor::state::InMemoryState>;
type GovernorClock = governor::clock::MonotonicClock;
type GovernorMiddleware = governor::middleware::NoOpMiddleware<std::time::Instant>;

const TG_RATE_LIMIT: u32 = 20;
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/87.0.4280.88 Safari/537.36 Edg/87.0.664.66";

#[derive(Debug)]
pub struct UploadedInfo {
    pub uploaded: Uploaded,
    pub file_name: String,
    pub file_size: usize,
    pub mime_type: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Hash)]
pub struct CommandCallback {
    pub category: String,
    pub action: String,
    pub page: u64,
    pub keyword: String,
    pub data: String,
}

impl CommandCallback {
    pub fn new(category: &str, action: &str, page: u64, keyword: String, data: String) -> Self {
        Self {
            category: category.to_owned(),
            action: action.to_owned(),
            page,
            keyword,
            data,
        }
    }
}

pub struct Bridge {
    pub admin_id: i64,
    pub bot_client: Client,
    pub db: DatabaseConnection,
    api_sender: mpsc::Sender<OnebotRequest>,

    remote_chat_cache: DashMap<RemoteChatKey, Arc<ChatModel>>,
    callback_cache: DashMap<String, CommandCallback>,
    tg_chat_cache: DashMap<(PackedType, i64), Arc<Chat>>,
    tg_rate_limit: Arc<RateLimiter<i64, GovernorStateMap, GovernorClock, GovernorMiddleware>>,
}

macro_rules! onebot_api {
    // 函数名, 返回类型枚举, 返回类型, 请求类型, 请求字段列表
    ($func_name:ident, $enum_variant:ident, $return_type:ty, $request_type:ident, $($param:ident: $param_type:ty),+) => {
        pub async fn $func_name(
            &self,
            endpoint: &Endpoint,
            $($param: $param_type),+
        ) -> Result<Arc<$return_type>> {
            let request_params = $request_type { $($param),+ };
            let request = Request::$func_name(request_params);

            match OnebotPylon::call_api(self.api_sender.clone(), endpoint.clone(), request).await {
                Ok(response) => {
                    if response.status.as_str() != "ok" {
                        return Err(anyhow::anyhow!(
                            "failed to {}, retcode: {}",
                            stringify!($func_name),
                            response.retcode
                        ));
                    }

                    match response.data.clone() {
                        ResponseData::$enum_variant(data) => Ok(data),
                        _ => Err(anyhow::anyhow!("invalid return data 1")),
                    }
                }
                Err(e) => Err(anyhow::anyhow!("failed to {}: {}", stringify!($func_name), e)),
            }
        }
    };
    // 函数名, 返回类型枚举, 返回类型
    ($func_name:ident, $enum_variant:ident, $return_type:ty) => {
        pub async fn $func_name(&self, endpoint: &Endpoint) -> Result<Arc<$return_type>> {
            match OnebotPylon::call_api(self.api_sender.clone(), endpoint.clone(), Request::$func_name()).await
            {
                Ok(response) => {
                    if response.status.as_str() != "ok" {
                        return Err(anyhow::anyhow!(
                            "failed to {}, retcode: {}",
                            stringify!($func_name),
                            response.retcode
                        ));
                    }

                    match response.data.clone() {
                        ResponseData::$enum_variant(data) => Ok(data),
                        _ => Err(anyhow::anyhow!("invalid return data 2")),
                    }
                }
                Err(e) => Err(anyhow::anyhow!(
                    "failed to {}: {}",
                    stringify!($func_name),
                    e
                )),
            }
        }
    };
}

macro_rules! onebot_api_no_resp {
    // 函数名, 请求类型, 请求字段列表
    ($func_name:ident, $request_type:ident, $($param:ident: $param_type:ty),+) => {
        pub async fn $func_name(
            &self,
            endpoint: &Endpoint,
            $($param: $param_type),+
        ) -> Result<()> {
            let request_params = $request_type { $($param),+ };
            let request = Request::$func_name(request_params);

            match OnebotPylon::call_api(self.api_sender.clone(), endpoint.clone(), request).await {
                Ok(response) => {
                    if response.status.as_str() != "ok" {
                        return Err(anyhow::anyhow!(
                            "failed to {}, retcode: {}",
                            stringify!($func_name),
                            response.retcode
                        ));
                    }

                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!("failed to {}: {}", stringify!($func_name), e)),
            }
        }
    };
    // 函数名
    ($func_name:ident) => {
        pub async fn $func_name(&self, endpoint: &Endpoint) -> Result<()> {
            match OnebotPylon::call_api(self.api_sender.clone(), endpoint.clone(), Request::$func_name()).await
            {
                Ok(response) => {
                    if response.status.as_str() != "ok" {
                        return Err(anyhow::anyhow!(
                            "failed to {}, retcode: {}",
                            stringify!($func_name),
                            response.retcode
                        ));
                    }

                    Ok(())
                }
                Err(e) => Err(anyhow::anyhow!(
                    "failed to {}: {}",
                    stringify!($func_name),
                    e
                )),
            }
        }
    };
}

macro_rules! download_seg{
    ($func_name:ident, $get_image_method:ident, $($param:ident: $type:ty),*) => {
        async fn $func_name(
            &self,
            endpoint: &Endpoint,
            $($param: $type),*
        ) -> Result<(String, Vec<u8>)> {
            let file_info = self.$get_image_method(endpoint, $($param),*).await?;
            if let Some(base64_data) = file_info.base64.as_ref() {
                return Ok((
                    file_info.file_name.clone(),
                    BASE64_STANDARD.decode(base64_data)?,
                ));
            }
            Err(anyhow::anyhow!("Failed to download image segment"))
        }
    };
}

macro_rules! save_remote_chat {
    ($func_name:ident, $info_type:ty, $chat_type:ident, $target_id:ident) => {
        async fn $func_name(
            &self,
            endpoint: &Endpoint,
            info: Arc<$info_type>,
        ) -> Result<ChatModel> {
            let model = entities::remote_chat::ActiveModel {
                endpoint: Set(endpoint.to_owned()),
                chat_type: Set(ChatType::$chat_type),
                target_id: Set(info.$target_id.to_owned()),
                name: Set(info.display_name()),
                ..Default::default()
            };
            Ok(model.insert(&self.db).await?)
        }
    };
}

macro_rules! update_remote_chat {
    ($func_name:ident, $info_type:ty, $chat_type:ident, $target_id:ident) => {
        pub async fn $func_name(&self, endpoint: &Endpoint, info: &$info_type) -> Result<()> {
            let timestamp = Utc::now().timestamp();
            let model = entities::remote_chat::ActiveModel {
                endpoint: Set(endpoint.to_owned()),
                chat_type: Set(ChatType::$chat_type),
                target_id: Set(info.$target_id.to_owned()),
                name: Set(info.display_name()),
                created_at: Set(timestamp),
                updated_at: Set(timestamp),
                ..Default::default()
            };

            entities::remote_chat::Entity::insert(model)
                .on_conflict(
                    sea_query::OnConflict::columns([
                        entities::remote_chat::Column::Endpoint,
                        entities::remote_chat::Column::ChatType,
                        entities::remote_chat::Column::TargetId,
                    ])
                    .update_columns([
                        entities::remote_chat::Column::Name,
                        entities::remote_chat::Column::UpdatedAt,
                    ])
                    .to_owned(),
                )
                .exec(&self.db)
                .await?;

            Ok(())
        }
    };
}

impl Bridge {
    pub fn new(
        admin_id: i64,
        bot_client: Client,
        db: DatabaseConnection,
        api_sender: mpsc::Sender<OnebotRequest>,
    ) -> Self {
        Self {
            admin_id,
            bot_client,
            db,
            api_sender,
            remote_chat_cache: DashMap::new(),
            callback_cache: DashMap::new(),
            tg_chat_cache: DashMap::new(),
            tg_rate_limit: Arc::new(RateLimiter::keyed(Quota::per_minute(
                NonZeroU32::new(TG_RATE_LIMIT - 1).unwrap(),
            ))),
        }
    }

    pub async fn send_telegram_message<
        C: Into<PackedChat>,
        M: Into<grammers_client::types::InputMessage>,
    >(
        &self,
        chat: C,
        message: M,
    ) -> Result<Message> {
        // 限制发送频率
        let chat: PackedChat = chat.into();
        self.tg_rate_limit.until_key_ready(&chat.id).await;

        Ok(self.bot_client.send_message(chat, message).await?)
    }

    pub async fn send_telegram_album<C: Into<PackedChat>>(
        &self,
        chat: C,
        medias: Vec<grammers_client::types::input_media::InputMedia>,
    ) -> Result<Vec<Option<Message>>> {
        // 限制发送频率
        let chat: PackedChat = chat.into();
        self.tg_rate_limit.until_key_ready(&chat.id).await;

        Ok(self.bot_client.send_album(chat, medias).await?)
    }

    pub async fn upload_segment(
        &self,
        endpoint: &Endpoint,
        segment: &Segment,
    ) -> Result<UploadedInfo> {
        let mut segment_data = self.download_segment(endpoint, segment).await?;

        let mut kind = infer::get(&segment_data.1);

        // TODO: 是不是所有的GIF都应该转成Sticker
        if ob_helper::is_sticker(segment) {
            if kind.filter(|i| i.mime_type() == "image/gif").is_some() {
                match ob_helper::gif_to_webm(&segment_data.1).await {
                    Ok(webm_data) => {
                        kind = infer::get(&webm_data);
                        segment_data.1 = webm_data;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to convert gif to webm: {}", e);
                    }
                }
            } else {
                match ob_helper::img_to_webp(&segment_data.1) {
                    Ok(webp_data) => {
                        kind = infer::get(&webp_data);
                        segment_data.1 = webp_data;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to convert image to webp: {}", e);
                    }
                }
            }
        } else if let Segment::Record(_) = segment {
            // QQ的目前是获取wav格式的, 需要转成opus ogg
            if let Platform::QQ = endpoint.platform {
                match ob_helper::wav_to_ogg(&segment_data.1).await {
                    Ok(ogg_data) => {
                        kind = infer::get(&ogg_data);
                        segment_data.1 = ogg_data;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to convert wav to ogg: {}", e);
                    }
                }
            }
        }

        let mut file_name = segment_data.0.clone();
        if let Some(info) = kind {
            if let Some(fixed_name) = fix_filename(&file_name, info.extension()) {
                file_name = fixed_name;
            }
        }

        let size = segment_data.1.len();
        let mut stream = std::io::Cursor::new(&segment_data.1);
        let uploaded = self
            .bot_client
            .upload_stream(&mut stream, size, file_name.clone())
            .await?;

        // TODO: 针对图片返回width和height
        let (width, height) = match segment {
            Segment::Image(_) => match kind {
                Some(info) => ob_helper::image_size(&segment_data.1, info.mime_type()),
                None => (0, 0),
            },
            _ => (0, 0),
        };

        Ok(UploadedInfo {
            uploaded,
            file_name,
            file_size: size,
            mime_type: match kind {
                Some(info) => info.mime_type().to_string(),
                None => "application/octet-stream".to_string(),
            },
            width,
            height,
        })
    }

    pub async fn get_remote_chat(
        &self,
        endpoint: &Endpoint,
        chat_type: &ChatType,
        target_id: &str,
    ) -> Result<Arc<ChatModel>> {
        let key = (endpoint.clone(), chat_type.clone(), target_id.to_owned());

        match self.remote_chat_cache.entry(key) {
            dashmap::Entry::Occupied(entry) => Ok(entry.get().clone()),
            dashmap::Entry::Vacant(entry) => {
                let model = entities::remote_chat::Entity::find()
                    .filter(entities::remote_chat::Column::Endpoint.eq(endpoint))
                    .filter(entities::remote_chat::Column::ChatType.eq(chat_type))
                    .filter(entities::remote_chat::Column::TargetId.eq(target_id))
                    .one(&self.db)
                    .await?;

                match model {
                    Some(model) => {
                        let value = Arc::new(model);
                        entry.insert(value.clone());
                        Ok(value)
                    }
                    None => match &chat_type {
                        ChatType::Private => {
                            let info = self
                                .get_stranger_info(endpoint, target_id.to_owned(), true)
                                .await?;
                            let model = self.save_remote_private_chat(endpoint, info).await?;
                            let value = Arc::new(model);
                            entry.insert(value.clone());
                            Ok(value)
                        }
                        ChatType::Group => {
                            let info = self
                                .get_group_info(endpoint, target_id.to_owned(), true)
                                .await?;
                            let model = self.save_remote_group_chat(endpoint, info).await?;
                            let value = Arc::new(model);
                            entry.insert(value.clone());
                            Ok(value)
                        }
                    },
                }
            }
        }
    }

    pub async fn get_tg_chat(&self, packed_type: PackedType, chat_id: i64) -> Result<Arc<Chat>> {
        match self.tg_chat_cache.entry((packed_type, chat_id)) {
            dashmap::Entry::Occupied(entry) => Ok(entry.get().clone()),
            dashmap::Entry::Vacant(entry) => {
                let packed_chat = PackedChat {
                    ty: packed_type,
                    id: chat_id,
                    access_hash: Some(0),
                };
                let chat = Arc::new(self.bot_client.unpack_chat(packed_chat).await?);
                entry.insert(chat.clone());
                Ok(chat)
            }
        }
    }

    pub async fn find_message_by_remote(
        &self,
        remote_chat_id: i64,
        message_id: &str,
    ) -> Result<Option<entities::message::Model>> {
        Ok(entities::message::Entity::find()
            .filter(entities::message::Column::RemoteChatId.eq(remote_chat_id))
            .filter(entities::message::Column::RemoteMsgId.eq(message_id))
            .one(&self.db)
            .await?)
    }

    pub async fn find_message_by_tg(
        &self,
        tg_chat_id: i64,
        tg_msg_id: i32,
    ) -> Result<
        Option<(
            entities::message::Model,
            Option<entities::remote_chat::Model>,
        )>,
    > {
        Ok(entities::message::Entity::find()
            .find_also_related(entities::remote_chat::Entity)
            .filter(entities::message::Column::TgChatId.eq(tg_chat_id))
            .filter(entities::message::Column::TgMsgId.eq(tg_msg_id))
            .one(&self.db)
            .await?)
    }

    pub async fn find_link_by_remote(
        &self,
        remote_chat_id: i64,
    ) -> Result<Option<entities::link::Model>> {
        Ok(entities::link::Entity::find()
            .filter(entities::link::Column::RemoteChatId.eq(remote_chat_id))
            .one(&self.db)
            .await?)
    }

    pub async fn find_link_by_tg(
        &self,
        tg_chat_id: i64,
    ) -> Result<Option<(entities::link::Model, Option<entities::remote_chat::Model>)>> {
        Ok(entities::link::Entity::find()
            .filter(entities::link::Column::TgChatId.eq(tg_chat_id))
            .find_also_related(entities::remote_chat::Entity)
            .one(&self.db)
            .await?)
    }

    pub async fn find_archive_by_endpoint(
        &self,
        endpoint: &Endpoint,
    ) -> Result<Option<entities::archive::Model>> {
        Ok(entities::archive::Entity::find()
            .filter(entities::archive::Column::Endpoint.eq(endpoint))
            .one(&self.db)
            .await?)
    }

    pub async fn find_archive_by_tg(
        &self,
        tg_chat_id: i64,
        tg_topic_id: i32,
    ) -> Result<Option<entities::remote_chat::Model>> {
        match entities::topic::Entity::find()
            .find_also_related(entities::archive::Entity)
            .find_also_related(entities::remote_chat::Entity)
            .filter(entities::topic::Column::TgTopicId.eq(tg_topic_id))
            .filter(entities::archive::Column::TgChatId.eq(tg_chat_id))
            .one(&self.db)
            .await?
        {
            Some((_, _, remote_chat)) => Ok(remote_chat),
            None => Ok(None),
        }
    }

    pub async fn create_archive(&self, endpoint: &Endpoint, tg_chat_id: i64) -> Result<()> {
        let entity = entities::archive::ActiveModel {
            endpoint: Set(endpoint.to_owned()),
            tg_chat_id: Set(tg_chat_id),
            ..Default::default()
        };
        entity.insert(&self.db).await?;

        Ok(())
    }

    pub async fn delete_archive(&self, id: i64) -> Result<()> {
        // 删除关联的Topic
        entities::topic::Entity::delete_many()
            .filter(entities::topic::Column::ArchiveId.eq(id))
            .exec(&self.db)
            .await?;

        // 删除Archive
        entities::archive::Entity::delete_by_id(id)
            .exec(&self.db)
            .await?;

        Ok(())
    }

    pub async fn get_or_create_topic(
        &self,
        archive: &entities::archive::Model,
        remote_chat: &entities::remote_chat::Model,
    ) -> Result<i32> {
        // 查找已有的Topic
        if let Some(topic) = entities::topic::Entity::find()
            .filter(entities::topic::Column::RemoteChatId.eq(remote_chat.id))
            .one(&self.db)
            .await?
        {
            return Ok(topic.tg_topic_id);
        }

        let tg_chat = self
            .get_tg_chat(PackedType::Megagroup, archive.tg_chat_id)
            .await?;

        // 创建Topic
        let req = tl::functions::channels::CreateForumTopic {
            channel: tl::enums::InputChannel::Channel(tl::types::InputChannel {
                channel_id: archive.tg_chat_id,
                access_hash: tg_chat.pack().access_hash.unwrap_or(0),
            }),
            title: match remote_chat.chat_type {
                ChatType::Private => format!("👤 {}", remote_chat.name.clone()),
                ChatType::Group => format!("👥 {}", remote_chat.name.clone()),
            },
            icon_color: None,
            icon_emoji_id: None,
            random_id: rand::random::<i64>(),
            send_as: None,
        };
        match self.bot_client.invoke(&req).await? {
            grammers_tl_types::enums::Updates::Updates(updates) => {
                for update in &updates.updates {
                    if let tl::enums::Update::NewChannelMessage(message) = update {
                        if let tl::enums::Message::Service(service) = &message.message {
                            if let tl::enums::MessageAction::TopicCreate(_) = service.action {
                                self.create_topic(archive.id, service.id, remote_chat.id)
                                    .await?;
                                return Ok(service.id);
                            }
                        }
                    }
                }
            }
            _ => return Err(anyhow::anyhow!("Unsupported update type")),
        }

        Err(anyhow::anyhow!("Failed to get or create topic"))
    }

    pub async fn create_link(
        &self,
        tg_chat_type: PackedType,
        tg_chat_id: i64,
        remote_chat_id: i64,
    ) -> Result<()> {
        let entity = entities::link::ActiveModel {
            tg_chat_type: Set(tg_chat_type as u8),
            tg_chat_id: Set(tg_chat_id),
            remote_chat_id: Set(remote_chat_id),
            ..Default::default()
        };
        entity.insert(&self.db).await?;

        Ok(())
    }

    pub async fn delete_link(&self, id: i64) -> Result<()> {
        entities::link::Entity::delete_by_id(id)
            .exec(&self.db)
            .await?;

        Ok(())
    }

    pub async fn save_message_by_remote(
        &self,
        remote_chat_id: i64,
        remote_message_id: &str,
        telegram_message: &Message,
    ) -> Result<()> {
        let entity = entities::message::ActiveModel {
            tg_chat_id: Set(telegram_message.chat().id()),
            tg_msg_id: Set(telegram_message.id()),
            remote_chat_id: Set(remote_chat_id),
            remote_msg_id: Set(remote_message_id.to_owned()),
            delivery_status: Set(DeliveryStatus::Sent),
            ..Default::default()
        };
        entity.insert(&self.db).await?;

        Ok(())
    }

    pub fn put_callback(&self, callback: &CommandCallback) -> String {
        let mut hasher = DefaultHasher::new();
        callback.hash(&mut hasher);
        let hash = hasher.finish().to_string();
        self.callback_cache.insert(hash.clone(), callback.clone());
        hash
    }

    pub fn get_callback(&self, hash: &str) -> Option<CommandCallback> {
        self.callback_cache.remove(hash).map(|(_, v)| v)
    }

    pub async fn download_media(
        &self,
        media: &grammers_client::types::Media,
    ) -> Result<(String, Vec<u8>)> {
        let mut file_bytes = Vec::new();
        let mut download = self.bot_client.iter_download(media);
        while let Some(chunk) = download.next().await? {
            file_bytes.extend(chunk);
        }

        let file_name = match media {
            grammers_client::types::Media::Photo(photo) => photo.id().to_string() + ".jpg",
            grammers_client::types::Media::Document(document) => {
                get_tg_doc_file_name(document, &file_bytes)
            }
            grammers_client::types::Media::Sticker(sticker) => {
                get_tg_doc_file_name(&sticker.document, &file_bytes)
            }
            _ => Default::default(),
        };

        Ok((file_name, file_bytes))
    }

    async fn create_topic(
        &self,
        archive_id: i64,
        tg_topic_id: i32,
        remote_chat_id: i64,
    ) -> Result<()> {
        let entity = entities::topic::ActiveModel {
            archive_id: Set(archive_id),
            tg_topic_id: Set(tg_topic_id),
            remote_chat_id: Set(remote_chat_id),
            ..Default::default()
        };
        entity.insert(&self.db).await?;

        Ok(())
    }

    async fn download_segment(
        &self,
        endpoint: &Endpoint,
        segment: &Segment,
    ) -> Result<(String, Vec<u8>)> {
        match segment {
            Segment::Image(seg) => {
                if seg.emoji_id.is_some() {
                    if let Some(url) = seg.url.as_ref().filter(|s| s.starts_with("http")) {
                        return self.fetch_file(url).await;
                    }
                }
                self.download_image(
                    endpoint,
                    seg.file.clone(),
                    seg.file.clone(),
                    seg.emoji_id.clone(),
                )
                .await
            }
            Segment::MarketFace(seg) => {
                if let Some(url) = seg.url.as_ref().filter(|s| s.starts_with("http")) {
                    self.fetch_file(url).await
                } else {
                    self.download_mface(
                        endpoint,
                        seg.emoji_id.clone(),
                        seg.emoji_id.clone(),
                        Some(seg.emoji_id.clone()),
                    )
                    .await
                }
            }
            Segment::Record(seg) => {
                // NapCat和LLOneBot的ogg格式用的是Vorbis而不是opus, 直接传Telegram有问题
                let out_format = match endpoint.platform {
                    Platform::QQ => "wav".to_string(),
                    _ => "ogg".to_string(),
                };
                self.download_record(endpoint, seg.file.clone(), out_format)
                    .await
            }
            Segment::Video(seg) => {
                self.download_video(endpoint, seg.file.clone(), seg.file.clone())
                    .await
            }
            Segment::File(seg) => {
                self.download_file(endpoint, seg.file.clone(), seg.file.clone())
                    .await
            }
            _ => Err(anyhow::anyhow!("Failed to download segment")),
        }
    }

    async fn fetch_file(&self, url: &str) -> Result<(String, Vec<u8>)> {
        let url = Url::parse(url)?;
        let client = reqwest::Client::builder().user_agent(USER_AGENT).build()?;
        let response = client.get(url.as_str()).send().await?;
        let filename = get_final_filename(response.headers(), &url);

        Ok((filename, response.bytes().await?.to_vec()))
    }
}

#[allow(dead_code)]
impl Bridge {
    download_seg!(download_image, get_image, file: String, file_id: String, emoji_id: Option<String>);
    download_seg!(download_mface, get_image, file: String, file_id: String, emoji_id: Option<String>);
    download_seg!(download_video, get_file, file: String, file_id: String);
    download_seg!(download_record, get_record, file: String, out_format: String);
    download_seg!(download_file, get_file, file: String, file_id: String);

    onebot_api!(get_login_info, UserInfo, UserInfo);
    onebot_api!(get_stranger_info, UserInfo, UserInfo, GetStrangerInfo, user_id: String, no_cache: bool);
    onebot_api!(get_group_info, GroupInfo, GroupInfo, GetGroupInfo, group_id: String, no_cache: bool);
    onebot_api!(get_friend_list, FriendList, Vec<UserInfo>);
    onebot_api!(get_group_list, GroupList, Vec<GroupInfo>);
    onebot_api!(get_group_member_list, GroupMemberList, Vec<MemberInfo>, GetGroupMemberList, group_id: String);
    onebot_api!(get_group_member_info, MemberInfo, MemberInfo, GetGroupMemberInfo, group_id: String, user_id: String, no_cache: bool);
    onebot_api!(get_record, FileInfo, FileInfo, GetRecord, file: String, out_format: String);
    onebot_api!(get_image, FileInfo, FileInfo, GetImage, file: String, file_id: String, emoji_id: Option<String>);
    onebot_api!(get_file, FileInfo, FileInfo, GetFile, file: String, file_id: String);
    onebot_api!(send_msg, MessageId, MessageId, SendMsg, message_type: String, group_id: Option<String>, user_id: Option<String>, message: Vec<Segment>);
    onebot_api_no_resp!(delete_msg, DeleteMsg, message_id: String);

    save_remote_chat!(save_remote_private_chat, UserInfo, Private, user_id);
    save_remote_chat!(save_remote_group_chat, GroupInfo, Group, group_id);
    update_remote_chat!(update_remote_private_chat, UserInfo, Private, user_id);
    update_remote_chat!(update_remote_group_chat, GroupInfo, Group, group_id);
}

pub fn fix_filename(filename: &str, ext: &str) -> Option<String> {
    let path = Path::new(filename);
    let mut new_path = path.to_path_buf();

    match path.extension().and_then(|s| s.to_str()) {
        Some(current_ext) if current_ext.eq_ignore_ascii_case(ext) => Some(filename.to_string()),
        _ => {
            new_path.set_extension(ext);
            new_path.to_str().map(|s| s.to_string())
        }
    }
}

fn get_final_filename(headers: &reqwest::header::HeaderMap, url: &Url) -> String {
    let name = extract_filename_from_headers(headers)
        .or_else(|| extract_filename_from_url(url))
        .unwrap_or_else(|| generate_default_filename(headers));

    let invalid_chars = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    name.replace(|c| invalid_chars.contains(&c), "_")
}

fn extract_filename_from_headers(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let content_disposition = headers.get(CONTENT_DISPOSITION)?.to_str().ok()?;

    // 解析 filename 或 filename*（支持 RFC 5987 编码）
    let mut filename = None;
    for part in content_disposition.split(';') {
        let part = part.trim();
        if part.starts_with("filename*=") {
            // 处理 UTF-8 编码（如：filename*=UTF-8''%C2%A3.txt）
            let encoded = part.splitn(3, '\'').nth(2)?;
            filename = percent_encoding::percent_decode_str(encoded)
                .decode_utf8()
                .ok()
                .map(Cow::into_owned);
            break;
        } else if part.starts_with("filename=") {
            // 处理普通文件名（可能带引号）
            let value = part.split('=').nth(1)?;
            filename = Some(value.trim_matches('"').to_string());
        }
    }
    filename
}

fn extract_filename_from_url(url: &Url) -> Option<String> {
    url.path_segments()?
        .filter(|s| !s.is_empty())
        .last()
        .map(|s| s.to_string())
}

fn generate_default_filename(headers: &reqwest::header::HeaderMap) -> String {
    let ext = headers
        .get(CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .and_then(|ct| mime_guess::get_mime_extensions_str(ct))
        .and_then(|exts| exts.first())
        .unwrap_or(&"bin");

    format!("data.{}", ext)
}

fn get_tg_doc_file_name(document: &Document, data: &[u8]) -> String {
    let mut file_name = document.name().to_string();
    if file_name.is_empty() {
        if let Some(mime) = document.mime_type() {
            let exts = guess_exts(mime);
            if exts.is_empty() {
                file_name = document.id().to_string();
            } else {
                file_name = document.id().to_string() + "." + &exts[0];
            }
        }
    }

    if Path::new(&file_name).extension().is_none() {
        if let Some(kind) = infer::get(data) {
            file_name = file_name + "." + kind.extension();
        }
    }

    file_name
}

fn guess_exts(content_type: &str) -> Vec<String> {
    let content_type = {
        // text/html
        let mut content_type = content_type.trim().to_string();

        // text/html; charset=utf-8
        let pattern = r"([^;]+)";
        let re = Regex::new(pattern)
            .context("invalid regex pattern")
            .context(pattern)
            .unwrap();

        if let Some(cap) = re.captures(&content_type) {
            if let Some(mime_type) = cap.get(1) {
                content_type = mime_type.as_str().trim().to_string();
            }
        }

        content_type
    };

    mime_guess::get_mime_extensions_str(&content_type).map_or_else(Vec::new, |exts| {
        exts.iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<String>>()
    })
}
