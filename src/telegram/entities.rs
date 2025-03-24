use std::str::FromStr;

use sea_orm::{
    ColIdx, DbErr, QueryResult, TryGetError, TryGetable, Value,
    prelude::StringLen,
    sea_query::{ArrayType, ColumnType, ValueType, ValueTypeErr},
};

use crate::common::Endpoint;
use crate::common::{ChatType, DeliveryStatus};

pub mod archive;
pub mod link;
pub mod message;
pub mod remote_chat;
pub mod topic;

impl remote_chat::Model {
    pub fn to_id(&self) -> (Endpoint, ChatType, String) {
        (
            self.endpoint.clone(),
            self.chat_type.clone(),
            self.target_id.clone(),
        )
    }
}

impl ValueType for Endpoint {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::String(Some(s)) => Self::from_str(&s).map_err(|_| ValueTypeErr),
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "string".to_string()
    }

    fn column_type() -> ColumnType {
        ColumnType::String(StringLen::None)
    }

    fn array_type() -> ArrayType {
        ArrayType::String
    }
}

impl TryGetable for Endpoint {
    fn try_get_by<I: ColIdx>(res: &QueryResult, index: I) -> Result<Self, TryGetError> {
        let value = String::try_get_by(res, index)?;
        Self::from_str(&value).map_err(|e| TryGetError::DbErr(DbErr::Type(e)))
    }
}

impl From<Endpoint> for Value {
    fn from(endpoint: Endpoint) -> Self {
        endpoint.to_string().into()
    }
}

impl From<&Endpoint> for Value {
    fn from(endpoint: &Endpoint) -> Self {
        endpoint.to_string().into()
    }
}

impl ValueType for ChatType {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::Int(Some(n)) => match n {
                0 => Ok(ChatType::Private),
                1 => Ok(ChatType::Group),
                _ => Err(ValueTypeErr),
            },
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "integer".to_string()
    }

    fn column_type() -> ColumnType {
        ColumnType::Integer
    }

    fn array_type() -> ArrayType {
        ArrayType::Int
    }
}

impl TryGetable for ChatType {
    fn try_get_by<I: ColIdx>(res: &QueryResult, index: I) -> Result<Self, TryGetError> {
        let value = res.try_get_by(index)?;
        match value {
            0 => Ok(ChatType::Private),
            1 => Ok(ChatType::Group),
            _ => Err(TryGetError::DbErr(DbErr::Type(format!(
                "Invalid ChatType: {}",
                value
            )))),
        }
    }
}

impl From<ChatType> for Value {
    fn from(chat_type: ChatType) -> Self {
        (chat_type as i32).into()
    }
}

impl From<&ChatType> for Value {
    fn from(chat_type: &ChatType) -> Self {
        (chat_type.to_owned() as i32).into()
    }
}

impl ValueType for DeliveryStatus {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::Int(Some(n)) => match n {
                0 => Ok(DeliveryStatus::Pending),
                1 => Ok(DeliveryStatus::Failed),
                2 => Ok(DeliveryStatus::Sent),
                3 => Ok(DeliveryStatus::Recalled),
                _ => Err(ValueTypeErr),
            },
            _ => Err(ValueTypeErr),
        }
    }

    fn type_name() -> String {
        "integer".to_string()
    }

    fn column_type() -> ColumnType {
        ColumnType::Integer
    }

    fn array_type() -> ArrayType {
        ArrayType::Int
    }
}

impl TryGetable for DeliveryStatus {
    fn try_get_by<I: ColIdx>(res: &QueryResult, index: I) -> Result<Self, TryGetError> {
        let value = res.try_get_by(index)?;
        match value {
            0 => Ok(DeliveryStatus::Pending),
            1 => Ok(DeliveryStatus::Failed),
            2 => Ok(DeliveryStatus::Sent),
            3 => Ok(DeliveryStatus::Recalled),
            _ => Err(TryGetError::DbErr(DbErr::Type(format!(
                "Invalid DeliveryStatus: {}",
                value
            )))),
        }
    }
}

impl From<DeliveryStatus> for Value {
    fn from(delivery_status: DeliveryStatus) -> Self {
        (delivery_status as i32).into()
    }
}
