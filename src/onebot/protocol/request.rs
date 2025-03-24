use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use super::segment::Segment;
use super::{id_deserializer, option_id_deserializer};

/// Onebot API 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum Request {
    /// 获取登录号信息
    #[serde(rename = "get_login_info")]
    GetLoginInfo { echo: String },

    /// 获取陌生人信息
    #[serde(rename = "get_stranger_info")]
    GetStrangerInfo {
        echo: String,
        params: GetStrangerInfo,
    },

    /// 获取群信息
    #[serde(rename = "get_group_info")]
    GetGroupInfo { echo: String, params: GetGroupInfo },

    /// 获取好友列表
    #[serde(rename = "get_friend_list")]
    GetFriendList { echo: String },

    /// 获取群列表
    #[serde(rename = "get_group_list")]
    GetGroupList { echo: String },

    /// 获取群成员列表
    #[serde(rename = "get_group_member_list")]
    GetGroupMemberList {
        echo: String,
        params: GetGroupMemberList,
    },

    /// 获取群成员信息
    #[serde(rename = "get_group_member_info")]
    GetGroupMemberInfo {
        echo: String,
        params: GetGroupMemberInfo,
    },

    /// 获取语音
    #[serde(rename = "get_record")]
    GetRecord { echo: String, params: GetRecord },

    /// 获取图片
    #[serde(rename = "get_image")]
    GetImage { echo: String, params: GetImage },

    /// 获取文件
    #[serde(rename = "get_file")]
    GetFile { echo: String, params: GetFile },

    /// 撤回消息
    #[serde(rename = "delete_msg")]
    DeleteMsg { echo: String, params: DeleteMsg },

    /// 发送消息
    #[serde(rename = "send_msg")]
    SendMsg { echo: String, params: SendMsg },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetStrangerInfo {
    /// 用户ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 是否不使用缓存
    pub no_cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetGroupInfo {
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 是否不使用缓存
    pub no_cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetGroupMemberList {
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetGroupMemberInfo {
    /// 群ID
    #[serde(deserialize_with = "id_deserializer")]
    pub group_id: String,
    /// 用户ID
    #[serde(deserialize_with = "id_deserializer")]
    pub user_id: String,
    /// 是否不使用缓存
    pub no_cache: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRecord {
    /// 文件路径
    pub file: String,
    /// 输出格式
    pub out_format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetImage {
    /// 文件路径
    pub file: String,
    /// 文件ID
    pub file_id: String,
    /// Emoji ID
    pub emoji_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetFile {
    /// 文件路径
    pub file: String,
    /// 文件ID
    pub file_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteMsg {
    /// 消息ID
    #[serde(deserialize_with = "id_deserializer")]
    pub message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendMsg {
    /// 消息类型(private/group)
    pub message_type: String,
    /// 用户ID
    #[serde(deserialize_with = "option_id_deserializer")]
    #[serde(default)]
    pub user_id: Option<String>,
    /// 群ID
    #[serde(deserialize_with = "option_id_deserializer")]
    #[serde(default)]
    pub group_id: Option<String>,
    /// 消息内容
    pub message: Vec<Segment>,
}

macro_rules! echo {
    ($($x: tt),*) => {
        pub fn get_echo(&self) -> String {
            match self {
                $(Request::$x {
                    echo: echo,
                    ..
                } => echo.clone(),)*
            }
        }
    };
}

macro_rules! no_params_builder {
    ($(($fn_name: ident, $req_type: tt)),*) => {
        $(pub fn $fn_name() -> Request {
            Request::$req_type {
                echo: generate_echo().to_string(),
            }
        })*
    };
}

macro_rules! params_builder {
    ($(($fn_name: ident, $req_type: tt)),*) => {
        $(pub fn $fn_name(params: $req_type) -> Request {
            Request::$req_type {
                params,
                echo: generate_echo().to_string(),
            }
        })*
    };
}

#[allow(dead_code)]
impl Request {
    echo!(
        GetLoginInfo,
        GetStrangerInfo,
        GetGroupInfo,
        GetFriendList,
        GetGroupList,
        GetGroupMemberList,
        GetGroupMemberInfo,
        GetRecord,
        GetImage,
        GetFile,
        DeleteMsg,
        SendMsg
    );

    no_params_builder!(
        (get_login_info, GetLoginInfo),
        (get_friend_list, GetFriendList),
        (get_group_list, GetGroupList)
    );

    params_builder!(
        (get_stranger_info, GetStrangerInfo),
        (get_group_info, GetGroupInfo),
        (get_group_member_list, GetGroupMemberList),
        (get_group_member_info, GetGroupMemberInfo),
        (get_record, GetRecord),
        (get_image, GetImage),
        (get_file, GetFile),
        (delete_msg, DeleteMsg),
        (send_msg, SendMsg)
    );
}

fn generate_echo() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
