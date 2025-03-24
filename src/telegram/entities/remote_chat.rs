use chrono::Utc;
use sea_orm::{
    ActiveModelBehavior, ActiveValue::Set, ConnectionTrait, DbErr, DerivePrimaryKey,
    DeriveRelation, EntityTrait, EnumIter, PrimaryKeyTrait, Related, RelationDef, RelationTrait,
    entity::prelude::DeriveEntityModel, prelude::async_trait,
};

use crate::common::{ChatType, Endpoint};

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "remote_chat")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub endpoint: Endpoint,
    pub chat_type: ChatType,
    pub target_id: String,
    pub name: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_one = "super::link::Entity")]
    Link,
    #[sea_orm(has_one = "super::topic::Entity")]
    Topic,
    #[sea_orm(has_many = "super::message::Entity")]
    Message,
}

impl Related<super::link::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Link.def()
    }
}

impl Related<super::topic::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Topic.def()
    }
}

impl Related<super::message::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Message.def()
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
