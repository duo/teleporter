use chrono::Utc;
use sea_orm::{
    ActiveModelBehavior, ActiveValue::Set, ConnectionTrait, DbErr, DerivePrimaryKey,
    DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait, Related, RelationDef, RelationTrait,
    entity::prelude::DeriveEntityModel, prelude::async_trait,
};

use crate::common::DeliveryStatus;

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "message")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub tg_chat_id: i64,
    pub tg_msg_id: i32,
    pub remote_chat_id: i64,
    pub remote_msg_id: String,
    pub content: String,
    pub delivery_status: DeliveryStatus,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::remote_chat::Entity",
        from = "Column::RemoteChatId",
        to = "super::remote_chat::Column::Id"
    )]
    RemoteChat,
}

impl Related<super::remote_chat::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RemoteChat.def()
    }
}

#[async_trait::async_trait]
impl ActiveModelBehavior for ActiveModel {
    async fn before_save<C>(mut self, _db: &C, insert: bool) -> Result<Self, DbErr>
    where
        C: ConnectionTrait,
    {
        let timestamp = Utc::now().timestamp();

        if insert {
            self.created_at = Set(timestamp);
        }

        self.updated_at = Set(timestamp);

        Ok(self)
    }
}

impl Entity {}
