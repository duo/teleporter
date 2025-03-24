use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::segment::Segment;
use super::{id_deserializer, option_id_deserializer};
use crate::common::ChatType;

/// Onebot 事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "post_type")] // 上报类型
pub enum Event {
    /// 消息事件
    #[serde(rename = "message")]
    Message(MessageEvent),

    /// 自身发送的消息事件
    #[serde(rename = "message_sent")]
    MessageSent(MessageEvent),

    /// 元事件
    #[serde(rename = "meta_event")]
    Meta(MetaEvent),

    /// 通知事件
    #[serde(rename = "notice")]
    Notice(NoticeEvent),

    /// 请求事件
    #[serde(rename = "request")]
    Request(RequestEvent),
}

impl Event {
    pub fn get_chat_type(&self) -> ChatType {
        match self {
            Event::Message(event) => event.get_chat_type(),
            Event::MessageSent(event) => event.get_chat_type(),
            Event::Meta(_) => ChatType::Private,
            Event::Notice(event) => event.get_chat_type(),
            Event::Request(_) => ChatType::Private,
        }
    }

    pub fn get_chat_id(&self) -> String {
        match self {
            Event::Message(event) => event.get_chat_id(),
            Event::MessageSent(event) => event.get_chat_id(),
            Event::Meta(_) => "meta".to_string(),
            Event::Notice(event) => event.get_chat_id(),
            Event::Request(_) => "request".to_string(),
        }
    }
}

/// 消息事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 消息类型 (private/group)
    pub message_type: String,
    /// 消息子类型 (private: friend/group/other, group: normal/anonymous/notice)
    pub sub_type: String,
    /// 消息ID
    #[serde(deserialize_with = "id_deserializer")]
    pub message_id: String,
    /// 群ID
    #[serde(deserialize_with = "option_id_deserializer")]
    #[serde(default)]
    pub group_id: Option<String>,
    /// 发送者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 目标ID (private,message_sent对方用户ID)
    #[serde(deserialize_with = "option_id_deserializer")]
    #[serde(default)]
    pub target_id: Option<String>,
    /// 消息内容
    pub message: Vec<Segment>,
    /// 匿名者
    pub anonymous: Option<Anonymous>,
    /// 发送人
    pub sender: Sender,
    /// 其它字段
    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

impl MessageEvent {
    pub fn get_chat_id(&self) -> String {
        match self.message_type.as_str() {
            "private" => match &self.target_id {
                Some(target_id) if !target_id.is_empty() => target_id.clone(),
                _ => self.user_id.clone(),
            },
            "group" => self.group_id.clone().unwrap(),
            _ => String::new(),
        }
    }
}

/// 发送人
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sender {
    /// 发送者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 发送者昵称
    pub nickname: String,
    /// 群名片/备注
    pub card: Option<String>,
    /// 群角色
    pub role: Option<String>,
}

impl Sender {
    pub fn display_name(&self) -> String {
        match &self.card {
            Some(card) if !card.is_empty() => card.clone(),
            _ => self.nickname.clone(),
        }
    }
}

/// 匿名者
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Anonymous {
    /// 匿名者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub id: String,
    /// 匿名者名称
    pub name: String,
    /// 匿名者flag
    pub flag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "meta_event_type")] // 元事件类型
pub enum MetaEvent {
    /// 生命周期事件
    #[serde(rename = "lifecycle")]
    Lifecycle(LifecycleEvent),

    /// 心跳事件
    #[serde(rename = "heartbeat")]
    Heartbeat(HeartbeatEvent),
}

/// 生命周期事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 事件子类型 (enable/disable/connect)
    pub sub_type: String,
}

/// 心跳事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 状态信息
    pub status: Status,
    /// 到下次心跳的间隔，单位毫秒
    pub interval: i64,
}

/// 运行状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    /// 是否在线，None 表示无法查询到在线状态
    pub online: Option<bool>,
    /// 状态是否符合预期
    pub good: bool,
}

/// 通知事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "notice_type")] // 通知类型
pub enum NoticeEvent {
    /// 好友消息撤回事件
    #[serde(rename = "friend_recall")]
    FriendRecall(FriendRecallEvent),

    /// 群消息撤回事件
    #[serde(rename = "group_recall")]
    GroupRecall(GroupRecallEvent),

    /// 提示事件
    #[serde(rename = "notify")]
    Notify(NotifyEvent),

    /// 群文件上传事件
    #[serde(rename = "group_upload")]
    GroupUpload(GroupUploadEvent),

    /// 群管理员变动事件
    #[serde(rename = "group_admin")]
    GroupAdmin(GroupAdminEvent),

    /// 群成员减少事件
    #[serde(rename = "group_decrease")]
    GroupDecrease(GroupDecreaseEvent),

    /// 群成员增加事件
    #[serde(rename = "group_increase")]
    GroupIncrease(GroupIncreaseEvent),

    /// 群名片事件
    #[serde(rename = "group_card")]
    GroupCard(GroupCardEvent),
}

impl NoticeEvent {
    pub fn get_chat_type(&self) -> ChatType {
        match self {
            NoticeEvent::FriendRecall(_) => ChatType::Private,
            NoticeEvent::GroupRecall(_) => ChatType::Group,
            NoticeEvent::Notify(e) => match &e.group_id {
                Some(group_id) if group_id != "0" => ChatType::Group,
                _ => ChatType::Private,
            },
            NoticeEvent::GroupUpload(_) => ChatType::Group,
            NoticeEvent::GroupAdmin(_) => ChatType::Group,
            NoticeEvent::GroupDecrease(_) => ChatType::Group,
            NoticeEvent::GroupIncrease(_) => ChatType::Group,
            NoticeEvent::GroupCard(_) => ChatType::Group,
        }
    }

    pub fn get_chat_id(&self) -> String {
        match self {
            NoticeEvent::FriendRecall(event) => event.get_chat_id(),
            NoticeEvent::GroupRecall(event) => event.get_chat_id(),
            NoticeEvent::Notify(e) => match &e.group_id {
                Some(group_id) if group_id != "0" => group_id.clone(),
                _ => e.user_id.clone().unwrap_or("0".to_string()),
            },
            NoticeEvent::GroupUpload(e) => e.group_id.clone(),
            NoticeEvent::GroupAdmin(e) => e.group_id.clone(),
            NoticeEvent::GroupDecrease(e) => e.group_id.clone(),
            NoticeEvent::GroupIncrease(e) => e.group_id.clone(),
            NoticeEvent::GroupCard(event) => event.group_id.clone(),
        }
    }
}

/// 好友消息撤回事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FriendRecallEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 消息ID
    #[serde(deserialize_with = "id_deserializer")]
    pub message_id: String,
    /// 发送者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
}

impl FriendRecallEvent {
    pub fn get_chat_id(&self) -> String {
        self.user_id.clone()
    }
}

/// 群消息撤回事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupRecallEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 消息ID
    #[serde(deserialize_with = "id_deserializer")]
    pub message_id: String,
    /// 发送者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 操作者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub operator_id: String,
}

impl GroupRecallEvent {
    pub fn get_chat_id(&self) -> String {
        self.group_id.clone()
    }
}

/// 提示事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotifyEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 提示类型 (poke, lucky_king, honor, input_status)
    pub sub_type: String,
    /// 发送者ID
    #[serde(deserialize_with = "option_id_deserializer")]
    pub user_id: Option<String>,
    /// 群ID
    #[serde(deserialize_with = "option_id_deserializer")]
    pub group_id: Option<String>,
    /// 其它字段
    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

/// 群名片事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupCardEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 发送者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 旧的群名片
    pub card_old: String,
    /// 新的群名片
    pub card_new: String,
}

/// 群文件上传事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupUploadEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 发送者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 其它字段
    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

/// 群管理员变动事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAdminEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 管理员ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 其它字段
    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

/// 群成员减少事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDecreaseEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 离开者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 其它字段
    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

/// 群成员增加事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupIncreaseEvent {
    /// 事件发生的时间戳
    pub time: i64,
    /// 收到事件的机器人ID
    #[serde(deserialize_with = "id_deserializer")]
    pub self_id: String,
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 加入者ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 其它字段
    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

/// 请求事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestEvent {}
