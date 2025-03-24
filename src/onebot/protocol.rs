use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Deserializer};
use tokio::sync::oneshot;

use crate::common::Endpoint;

use event::Event;
use request::Request;
use response::Response;

pub mod event;
pub mod payload;
pub mod request;
pub mod response;
pub mod segment;

pub struct OnebotEvent {
    // 端点
    pub endpoint: Endpoint,
    // 事件
    pub raw: Arc<Event>,
}

pub struct OnebotRequest {
    // 端点
    pub endpoint: Endpoint,
    // 请求
    pub raw: Arc<Request>,
    // 响应通道
    pub ret: oneshot::Sender<Result<Arc<Response>>>,
}

pub fn id_deserializer<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    // 直接解析为 serde_json::Value（不是 Option）
    match serde_json::Value::deserialize(deserializer)? {
        // 处理数字类型（整数或浮点数）
        serde_json::Value::Number(n) => Ok(n.to_string()),
        // 处理字符串类型
        serde_json::Value::String(s) => Ok(s),
        // 其他类型（包括 null）返回错误
        other => Err(serde::de::Error::custom(format!(
            "expected number or string, found {}",
            other
        ))),
    }
}

pub fn option_id_deserializer<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    // 尝试将输入数据解析为 Option<serde_json::Value>
    match Option::<serde_json::Value>::deserialize(deserializer)? {
        // 处理数字类型（整数或浮点数）
        Some(serde_json::Value::Number(n)) => Ok(Some(n.to_string())),
        // 处理字符串类型
        Some(serde_json::Value::String(s)) => Ok(Some(s)),
        // 处理 null 或字段不存在的情况
        None => Ok(None),
        // 其他类型返回错误
        Some(other) => Err(serde::de::Error::custom(format!(
            "expected number, string, or null, found {}",
            other
        ))),
    }
}
