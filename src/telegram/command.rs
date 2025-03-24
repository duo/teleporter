use std::collections::HashMap;

use anyhow::Result;
use grammers_client::types::{CallbackQuery, Chat, Message};
use grammers_client::{InputMessage, button, reply_markup};
use grammers_tl_types as tl;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect};

use super::bridge::{Bridge, CommandCallback};
use super::{entities, telegram_helper as tg_helper};
use crate::TelegramPylon;
use crate::common::{ChatType, Endpoint};

const PAGE_SIZE: u64 = 10;
const PLACE_HOLDER: &str = "porter";

impl TelegramPylon {
    pub async fn process_callback(bridge: &Bridge, callback: &CallbackQuery) -> Result<()> {
        let message = callback.load_message().await?;
        if let Some(command_callback) =
            bridge.get_callback(std::str::from_utf8(callback.data()).unwrap_or(""))
        {
            match command_callback.category.as_str() {
                "archive" => match command_callback.action.as_str() {
                    "create" => Self::create_archive(bridge, &message, &command_callback).await?,
                    "delete" => Self::delete_archive(bridge, &message, &command_callback).await?,
                    "cancel" => Self::cancel_archive(bridge, &message, &command_callback).await?,
                    _ => {}
                },
                "link" => match command_callback.action.as_str() {
                    "create" => Self::create_link(bridge, &message, &command_callback).await?,
                    "delete" => Self::delete_link(bridge, &message, &command_callback).await?,
                    "list" => Self::list_link(bridge, &message, &command_callback).await?,
                    "cancel" => Self::cancel_link(bridge, &message, &command_callback).await?,
                    _ => {}
                },
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn process_command(bridge: &Bridge, message: &Message, command: &str) -> Result<()> {
        if !tg_helper::check_sender(bridge, message) {
            return Ok(());
        }

        match command {
            "/help" => {
                message
                    .respond(InputMessage::html(
                        "help - Show command list.\n\
                        link - Manage remote chat link.\n\
                        archive - Archive remote chat.",
                    ))
                    .await?;
            }
            "/archive" => {
                if let Chat::Group(group) = message.chat() {
                    if let tl::enums::Chat::Channel(channel) = group.raw {
                        if channel.megagroup && channel.forum {
                            return Self::process_archive(bridge, message).await;
                        }
                    }
                }
                message
                    .respond(InputMessage::html(
                        "<b>Currently, archive is only supported in forum groups</b>",
                    ))
                    .await?;
            }
            "/link" => {
                if let Chat::Group(group) = message.chat() {
                    match group.raw {
                        tl::enums::Chat::Chat(_) => {
                            return Self::process_link(bridge, message).await;
                        }
                        tl::enums::Chat::Channel(channel) => {
                            // 目前不支持绑定在有Topic的群
                            if channel.megagroup && !channel.forum {
                                return Self::process_link(bridge, message).await;
                            }
                        }
                        _ => {}
                    }
                }
                message
                    .respond(InputMessage::html(
                        "<b>Currently, link creation is only supported in regular groups</b>",
                    ))
                    .await?;
            }
            _ => {
                message
                    .respond(InputMessage::html("<b>Command not supported</b>"))
                    .await?;
            }
        }

        Ok(())
    }

    async fn process_archive(bridge: &Bridge, message: &Message) -> Result<()> {
        Self::list_archive(bridge, message).await
    }

    async fn create_archive(
        bridge: &Bridge,
        message: &Message,
        callback: &CommandCallback,
    ) -> Result<()> {
        match callback.data.parse::<Endpoint>() {
            // TODO: 是否把原先的解绑然后重新绑定到当前的?还是仅仅提示绑定失败
            Ok(endpoint) => match bridge.create_archive(&endpoint, message.chat().id()).await {
                Ok(_) => tracing::info!("Created archive successfully"),
                Err(e) => tracing::warn!("Failed to create archive: {:?}", e),
            },
            Err(_) => tracing::warn!("Invalid endpoint: {:?}", callback.data),
        }

        Self::list_archive(bridge, message).await
    }

    async fn delete_archive(
        bridge: &Bridge,
        message: &Message,
        callback: &CommandCallback,
    ) -> Result<()> {
        match callback.data.parse::<i64>() {
            Ok(id) => match bridge.delete_archive(id).await {
                Ok(_) => tracing::info!("Deleted archive successfully"),
                Err(e) => tracing::warn!("Failed to delete archive: {:?}", e),
            },
            Err(_) => tracing::warn!("Invalid archive id: {:?}", callback.data),
        }

        Self::list_archive(bridge, message).await
    }

    async fn cancel_archive(_: &Bridge, message: &Message, _: &CommandCallback) -> Result<()> {
        Ok(message
            .edit(InputMessage::html("<del>Cancelled by the user</del>"))
            .await?)
    }

    async fn list_archive(bridge: &Bridge, message: &Message) -> Result<()> {
        let tg_chat_id = message.chat().id();

        let mut content = "Archive: ".to_string();

        let mut archives: HashMap<Endpoint, entities::archive::Model> = HashMap::new();
        for archive in entities::archive::Entity::find().all(&bridge.db).await? {
            if archive.tg_chat_id == tg_chat_id {
                content.push_str(archive.endpoint.to_string().as_str());
            }
            archives.insert(archive.endpoint.clone(), archive);
        }

        let endpoints = entities::remote_chat::Entity::find()
            .select_only()
            .column(entities::remote_chat::Column::Endpoint)
            .distinct()
            .into_tuple::<Endpoint>()
            .all(&bridge.db)
            .await?;

        let mut markup = Vec::new();

        // 构建 endpoint 的列表
        for enpoint in &endpoints {
            let text = format!(
                "{}{}",
                match archives.get(enpoint) {
                    Some(_) => "🗃",
                    None => "",
                },
                enpoint
            );
            let cb = match archives.get(enpoint) {
                Some(archive) => CommandCallback::new(
                    "archive",
                    "delete",
                    0,
                    String::new(),
                    archive.id.to_string(),
                ),
                None => {
                    CommandCallback::new("archive", "create", 0, String::new(), enpoint.to_string())
                }
            };

            markup.push(vec![button::inline(text, bridge.put_callback(&cb))]);
        }

        // 构造取消按钮
        {
            let cb = CommandCallback::new("archive", "cancel", 0, String::new(), String::new());
            markup.push(vec![button::inline(
                "cancel".to_string(),
                bridge.put_callback(&cb),
            )]);
        }

        // 如果源消息是Bot发送的，直接编辑源消息, 否则回复一条新消息
        if message.outgoing() {
            message
                .edit(InputMessage::text(content).reply_markup(&reply_markup::inline(markup)))
                .await?;
        } else {
            message
                .respond(InputMessage::text(content).reply_markup(&reply_markup::inline(markup)))
                .await?;
        }

        Ok(())
    }

    async fn process_link(bridge: &Bridge, message: &Message) -> Result<()> {
        let callback = CommandCallback::new(
            "link",
            "list",
            0,
            message.text()[5..].trim().to_owned(),
            String::new(),
        );

        Self::list_link(bridge, message, &callback).await
    }

    async fn create_link(
        bridge: &Bridge,
        message: &Message,
        callback: &CommandCallback,
    ) -> Result<()> {
        match callback.data.parse::<i64>() {
            // TODO: 是否把原先的解绑然后重新绑定到当前的?还是仅仅提示绑定失败
            Ok(remote_chat_id) => match bridge
                .create_link(
                    tg_helper::get_packed_type(message),
                    message.chat().id(),
                    remote_chat_id,
                )
                .await
            {
                Ok(_) => tracing::info!("Created link successfully"),
                Err(e) => tracing::warn!("Failed to create link: {:?}", e),
            },
            Err(_) => tracing::warn!("Invalid remote chat id: {:?}", callback.data),
        }

        Self::list_link(bridge, message, callback).await
    }

    async fn delete_link(
        bridge: &Bridge,
        message: &Message,
        callback: &CommandCallback,
    ) -> Result<()> {
        match callback.data.parse::<i64>() {
            Ok(id) => match bridge.delete_link(id).await {
                Ok(_) => tracing::info!("Deleted link successfully"),
                Err(e) => tracing::warn!("Failed to delete link: {:?}", e),
            },
            Err(_) => tracing::warn!("Invalid link id: {:?}", callback.data),
        }

        Self::list_link(bridge, message, callback).await
    }

    async fn cancel_link(_: &Bridge, message: &Message, _: &CommandCallback) -> Result<()> {
        Ok(message
            .edit(InputMessage::html("<del>Cancelled by the user</del>"))
            .await?)
    }

    async fn list_link(
        bridge: &Bridge,
        message: &Message,
        callback: &CommandCallback,
    ) -> Result<()> {
        let page = callback.page;
        let keyword = callback.keyword.clone();

        let mut query =
            entities::remote_chat::Entity::find().find_also_related(entities::link::Entity);
        // 添加过滤条件
        if !callback.keyword.is_empty() {
            query = query
                .filter(entities::remote_chat::Column::Name.like(format!("%{}%", keyword.clone())));
        }

        let chat_pages = query
            .order_by_asc(entities::remote_chat::Column::Id)
            .paginate(&bridge.db, PAGE_SIZE);

        // 获取分页信息
        let pagination_info = chat_pages.num_items_and_pages().await?;
        if pagination_info.number_of_items == 0 {
            let msg = InputMessage::html("<b>There are no remote chats avaiable</b>");
            // 如果源消息是Bot发送的，直接编辑源消息, 否则回复一条新消息
            if message.outgoing() {
                message.edit(msg).await?;
            } else {
                message.respond(msg).await?;
            }
            return Ok(());
        }

        // 获取当前链接信息
        let content = match entities::link::Entity::find()
            .find_also_related(entities::remote_chat::Entity)
            .filter(entities::link::Column::TgChatId.eq(message.chat().id()))
            .one(&bridge.db)
            .await?
        {
            Some((_, Some(remote_chat))) => format!(
                "Link: 🔗{}({}) from ({})",
                remote_chat.name, remote_chat.target_id, remote_chat.endpoint
            ),
            _ => "Link:".to_string(),
        };

        let mut markup = Vec::new();

        // 构建 remote chat 的列表
        for (chat, link) in &chat_pages.fetch_page(page).await? {
            let text = format!(
                "{}{}{}({}) from ({})",
                match link {
                    Some(_) => "🔗",
                    None => "",
                },
                match chat.chat_type {
                    ChatType::Private => "👤",
                    ChatType::Group => "👥",
                },
                chat.name,
                chat.target_id,
                chat.endpoint
            );
            let cb = match link {
                Some(link) => CommandCallback::new(
                    "link",
                    "delete",
                    page,
                    keyword.clone(),
                    link.id.to_string(),
                ),
                None => CommandCallback::new(
                    "link",
                    "create",
                    page,
                    keyword.clone(),
                    chat.id.to_string(),
                ),
            };
            markup.push(vec![button::inline(text, bridge.put_callback(&cb))]);
        }

        // 构建分页按钮
        let mut bottom = Vec::new();
        if page > 0 {
            let cb = CommandCallback::new(
                "link",
                "list",
                page - 1,
                keyword.clone(),
                callback.data.clone(),
            );
            bottom.push(button::inline("< Prev", bridge.put_callback(&cb)));
        } else {
            bottom.push(button::inline(" ", PLACE_HOLDER));
        }
        {
            let text = format!("{}/{} | Cancel", page + 1, pagination_info.number_of_pages);
            let cb = CommandCallback::new("link", "cancel", page, keyword.clone(), String::new());
            bottom.push(button::inline(text, bridge.put_callback(&cb)));
        }
        if page < pagination_info.number_of_pages - 1 {
            let cb = CommandCallback::new(
                "link",
                "list",
                page + 1,
                keyword.clone(),
                callback.data.clone(),
            );
            bottom.push(button::inline("Next >", bridge.put_callback(&cb)));
        } else {
            bottom.push(button::inline(" ", PLACE_HOLDER));
        }
        markup.push(bottom);

        // 如果源消息是Bot发送的，直接编辑源消息, 否则回复一条新消息
        if message.outgoing() {
            message
                .edit(InputMessage::text(content).reply_markup(&reply_markup::inline(markup)))
                .await?;
        } else {
            message
                .respond(InputMessage::text(content).reply_markup(&reply_markup::inline(markup)))
                .await?;
        }

        Ok(())
    }
}
