use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::event;
use super::request;
use super::response;

/// Onebot 负载, 包含请求、响应、事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Payload {
    /// 请求
    Request(Arc<request::Request>),

    /// 响应
    Response(Arc<response::Response>),

    /// 事件
    Event(Arc<event::Event>),
}
