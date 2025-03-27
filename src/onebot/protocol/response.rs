use std::{collections::HashMap, sync::Arc};

use serde::{Deserialize, Serialize};

use super::{event::MessageEvent, id_deserializer};

/// Onebot API 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub echo: String,
    /// 状态 (ok/async/failed)
    pub status: String,
    /// 状态码
    pub retcode: i32,
    /// 返回的数据
    pub data: ResponseData,
}

/// Onebot API 响应数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseData {
    /// delete_msg 响应数据
    None,

    /// send_msg 响应数据
    MessageId(Arc<MessageId>),

    /// get_group_member_info 响应数据
    MemberInfo(Arc<MemberInfo>),

    /// get_login_info, get_stranger_info, 响应数据
    UserInfo(Arc<UserInfo>),

    /// get_group_info 响应数据
    GroupInfo(Arc<GroupInfo>),

    /// get_group_member_list 响应数据
    GroupMemberList(Arc<Vec<MemberInfo>>),

    /// get_friend_list 响应数据
    FriendList(Arc<Vec<UserInfo>>),

    /// get_group_list 响应数据
    GroupList(Arc<Vec<GroupInfo>>),

    /// get_image, get_record, get_file 响应数据
    FileInfo(Arc<FileInfo>),

    /// get_forward_msg 响应数据
    ForwardMessage(Arc<ForwardMessage>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageId {
    /// 消息ID
    #[serde(deserialize_with = "id_deserializer")]
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    /// 用户ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 昵称
    pub nickname: String,
    /// 备注
    pub remark: Option<String>,
    /// 头像URL
    pub avatar: Option<String>,
}

impl UserInfo {
    pub fn display_name(&self) -> String {
        match &self.remark {
            Some(remark) if !remark.is_empty() => remark.clone(),
            _ => self.nickname.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 群名
    pub group_name: String,
    /// 头像URL
    pub avatar: Option<String>,
}

impl GroupInfo {
    pub fn display_name(&self) -> String {
        self.group_name.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberInfo {
    /// 用户ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 昵称
    pub nickname: String,
    /// 群名片/备注
    pub card: Option<String>,
    /// 角色 (owner/admin/member)
    pub role: String,
    /// 头像URL
    pub avatar: Option<String>,
    /// 其它字段
    #[serde(flatten)]
    pub extra_fields: HashMap<String, serde_json::Value>,
}

impl MemberInfo {
    pub fn display_name(&self) -> String {
        match &self.card {
            Some(card) if !card.is_empty() => card.clone(),
            _ => self.nickname.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInfo {
    /// 文件路径
    pub file: String,
    /// 文件名
    pub file_name: String,
    /// 文件大小
    pub file_size: Option<String>,
    /// 文件URL
    pub url: Option<String>,
    /// Base64编码的文件内容
    pub base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardMessage {
    /// 消息列表
    pub messages: Vec<MessageEvent>,
}
