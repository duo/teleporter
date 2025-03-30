use sea_orm::{
    DbErr, DeriveIden, DeriveMigrationName,
    prelude::async_trait,
    sea_query::{Index, Table},
};
use sea_orm_migration::{
    MigrationTrait, MigratorTrait, SchemaManager,
    schema::{integer, pk_auto, string},
};

#[derive(DeriveMigrationName)]
pub struct CreateTableMigration;

#[derive(DeriveIden)]
enum Archive {
    Table,
    Id,
    Endpoint,
    TgChatId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum RemoteChat {
    Table,
    Id,
    Endpoint,
    ChatType,
    TargetId,
    Name,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Link {
    Table,
    Id,
    TgChatType,
    TgChatId,
    RemoteChatId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Topic {
    Table,
    Id,
    ArchiveId,
    TgTopicId,
    RemoteChatId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Message {
    Table,
    Id,
    TgChatId,
    TgMsgId,
    RemoteChatId,
    RemoteMsgId,
    Content,
    DeliveryStatus,
    CreatedAt,
    UpdatedAt,
}

#[async_trait::async_trait]
impl MigrationTrait for CreateTableMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // 创建表
        manager
            .create_table(
                Table::create()
                    .table(Archive::Table)
                    .if_not_exists()
                    .col(pk_auto(Archive::Id))
                    .col(string(Archive::Endpoint))
                    .col(integer(Archive::TgChatId))
                    .col(integer(Archive::CreatedAt))
                    .col(integer(Archive::UpdatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(RemoteChat::Table)
                    .if_not_exists()
                    .col(pk_auto(RemoteChat::Id))
                    .col(string(RemoteChat::Endpoint))
                    .col(integer(RemoteChat::ChatType))
                    .col(string(RemoteChat::TargetId))
                    .col(string(RemoteChat::Name))
                    .col(integer(RemoteChat::CreatedAt))
                    .col(integer(RemoteChat::UpdatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Link::Table)
                    .if_not_exists()
                    .col(pk_auto(Link::Id))
                    .col(integer(Link::TgChatType))
                    .col(integer(Link::TgChatId))
                    .col(integer(Link::RemoteChatId))
                    .col(integer(Link::CreatedAt))
                    .col(integer(Link::UpdatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Topic::Table)
                    .if_not_exists()
                    .col(pk_auto(Topic::Id))
                    .col(integer(Topic::ArchiveId))
                    .col(integer(Topic::TgTopicId))
                    .col(integer(Topic::RemoteChatId))
                    .col(integer(Topic::CreatedAt))
                    .col(integer(Topic::UpdatedAt))
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(Message::Table)
                    .if_not_exists()
                    .col(pk_auto(Message::Id))
                    .col(integer(Message::TgChatId))
                    .col(integer(Message::TgMsgId))
                    .col(integer(Message::RemoteChatId))
                    .col(string(Message::RemoteMsgId))
                    .col(string(Message::Content))
                    .col(integer(Message::DeliveryStatus))
                    .col(integer(Message::CreatedAt))
                    .col(integer(Message::UpdatedAt))
                    .to_owned(),
            )
            .await?;

        // 创建索引
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("archive_unq_endpoint")
                    .table(Archive::Table)
                    .col(Archive::Endpoint)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("remote_chat_unq_chat")
                    .table(RemoteChat::Table)
                    .col(RemoteChat::Endpoint)
                    .col(RemoteChat::ChatType)
                    .col(RemoteChat::TargetId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("remote_chat_idx_name")
                    .table(RemoteChat::Table)
                    .col(RemoteChat::Name)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("link_unq_tg_chat")
                    .table(Link::Table)
                    .col(Link::TgChatId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("link_unq_remote_chat")
                    .table(Link::Table)
                    .col(Link::RemoteChatId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("topic_idx_archive")
                    .table(Topic::Table)
                    .col(Topic::ArchiveId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("topic_unq_remote_chat")
                    .table(Topic::Table)
                    .col(Topic::RemoteChatId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("message_unq_tg_msg")
                    .table(Message::Table)
                    .col(Message::TgChatId)
                    .col(Message::TgMsgId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .unique()
                    .name("message_unq_remote_msg")
                    .table(Message::Table)
                    .col(Message::RemoteChatId)
                    .col(Message::RemoteMsgId)
                    .col(Message::TgChatId)
                    .col(Message::TgMsgId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Message::Table).to_owned())
            .await?;

        Ok(())
    }
}

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![Box::new(CreateTableMigration)]
    }
}
