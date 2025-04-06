use core::fmt;
use core::hash::Hash;
use std::str::FromStr;

use config::Config;
use serde::Deserialize;

pub static CONFIG_PATH: &str = "config.toml";

pub type RemoteChatKey = (Endpoint, ChatType, String);

/// Teleporter 配置
#[derive(Debug, Deserialize)]
pub struct TeleporterConfig {
    pub telegram: TelegramConfig,
    pub onebot: OnebotConfig,
    pub general: GeneralConfig,
}

/// Telegram 配置
#[derive(Debug, Deserialize)]
pub struct TelegramConfig {
    /// Telegram Admin User ID
    pub admin_id: i64,
    /// Telegram Application API ID
    pub api_id: i32,
    /// Telegram Application API hash
    pub api_hash: String,
    /// Telegram Bot token
    pub bot_token: String,
    // Socks5 proxy url
    pub proxy_url: Option<String>,
    // Enable search
    pub enable_search: bool,
}

/// Onebot 配置
#[derive(Debug, Deserialize)]
pub struct OnebotConfig {
    /// WebSocket 监听地址
    pub addr: String,
    /// 连接验证 token
    pub token: Option<String>,
}

/// 通用配置
#[derive(Debug, Deserialize)]
pub struct GeneralConfig {
    /// 日志级别
    pub log_level: String,
}

impl TeleporterConfig {
    pub fn load() -> Self {
        let config = Config::builder()
            .add_source(config::File::with_name(CONFIG_PATH))
            .build()
            .unwrap();

        config.try_deserialize().unwrap()
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Platform {
    Telegram,
    QQ,
    WeChat,
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Platform::Telegram => f.write_str("telegram"),
            Platform::QQ => f.write_str("qq"),
            Platform::WeChat => f.write_str("wechat"),
        }
    }
}

impl FromStr for Platform {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "telegram" => Ok(Platform::Telegram),
            "qq" => Ok(Platform::QQ),
            "wechat" => Ok(Platform::WeChat),
            _ => Err(format!("invalid platform: {}", s)),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct Endpoint {
    pub platform: Platform,
    pub id: String,
}

impl Default for Endpoint {
    fn default() -> Self {
        Self {
            platform: Platform::Telegram,
            id: String::new(),
        }
    }
}

impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.platform.to_string())?;
        f.write_str(":")?;
        f.write_str(&self.id)
    }
}

impl FromStr for Endpoint {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (platform_str, id) = s
            .split_once(':')
            .ok_or_else(|| format!("invalid endpoint format: {}", s))?;

        let platform =
            Platform::from_str(platform_str).map_err(|_| format!("invalid platform: {}", s))?;

        Ok(Endpoint {
            platform,
            id: id.to_string(),
        })
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum ChatType {
    Private,
    Group,
}

impl fmt::Display for ChatType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ChatType::Private => f.write_str("private"),
            ChatType::Group => f.write_str("group"),
        }
    }
}

impl FromStr for ChatType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "private" => Ok(ChatType::Private),
            "group" => Ok(ChatType::Group),
            _ => Err(format!("invalid chat type: {}", s)),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum DeliveryStatus {
    Pending,
    Failed,
    Sent,
    Recalled,
}

impl fmt::Display for DeliveryStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DeliveryStatus::Pending => f.write_str("pending"),
            DeliveryStatus::Failed => f.write_str("failed"),
            DeliveryStatus::Sent => f.write_str("sent"),
            DeliveryStatus::Recalled => f.write_str("recalled"),
        }
    }
}

impl FromStr for DeliveryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(DeliveryStatus::Pending),
            "failed" => Ok(DeliveryStatus::Failed),
            "sent" => Ok(DeliveryStatus::Sent),
            "recalled" => Ok(DeliveryStatus::Recalled),
            _ => Err(format!("invalid delivery status: {}", s)),
        }
    }
}
