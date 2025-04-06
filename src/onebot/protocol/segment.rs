use serde::{Deserialize, Serialize};

/// Onebot 消息段类型
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum Segment {
    /// 纯文本
    #[serde(rename = "text")]
    Text(Text),

    /// 表情
    #[serde(rename = "face")]
    Face(Face),

    /// 商城表情
    #[serde(rename = "mface")]
    MarketFace(MarketFace),

    /// 图片
    #[serde(rename = "image")]
    Image(Image),

    /// 语音
    #[serde(rename = "record")]
    Record(Record),

    /// 视频
    #[serde(rename = "video")]
    Video(Video),

    /// 文件
    #[serde(rename = "file")]
    File(File),

    /// @某人
    #[serde(rename = "at")]
    At(At),

    /// 猜拳魔法表情
    #[serde(rename = "rps")]
    Rps,

    /// 掷骰子魔法表情
    #[serde(rename = "dice")]
    Dice,

    /// 窗口抖动
    #[serde(rename = "shake")]
    Shake,

    /// 戳一戳
    #[serde(rename = "poke")]
    Poke(Poke),

    /// 匿名发消息
    #[serde(rename = "anonymous")]
    Anonymous,

    /// 链接分享
    #[serde(rename = "share")]
    Share(Share),

    /// 推荐
    #[serde(rename = "contact")]
    Contact(Contact),

    /// 位置
    #[serde(rename = "location")]
    Location(Location),

    /// 音乐分享
    #[serde(rename = "music")]
    Music(Music),

    /// 回复
    #[serde(rename = "reply")]
    Reply(Reply),

    /// 合并转发
    #[serde(rename = "forward")]
    Forward(Forward),

    /// 并转发节点
    #[serde(rename = "node")]
    Node(Node),

    /// XML消息
    #[serde(rename = "xml")]
    Xml(Xml),

    /// JSON 消息
    #[serde(rename = "json")]
    Json(Json),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Text {
    /// 纯文本内容
    pub text: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Face {
    /// 表情ID
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarketFace {
    /// 商城表情ID
    pub emoji_id: String,
    /// 商城表情URL
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Image {
    /// 图片文件路径
    pub file: String,
    /// 图片文件名
    pub name: Option<String>,
    /// 图片URL
    pub url: Option<String>,
    /// 图片概述
    pub summary: Option<String>,
    /// Emoji图片ID
    pub emoji_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Record {
    /// 语音文件路径
    pub file: String,
    /// 语音文件名
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Video {
    /// 视频文件路径
    pub file: String,
    /// 视频文件名
    pub name: Option<String>,
    /// 视频URL
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct File {
    /// 文件路径
    pub file: String,
    /// 文件名
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct At {
    /// @某人的ID
    #[serde(rename = "qq")]
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Poke {
    /// 类型
    #[serde(rename = "type")]
    pub type_: String,
    /// ID
    pub id: String,
    /// 表情名
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Share {
    /// URL
    pub url: String,
    /// 标题
    pub title: String,
    /// 内容
    pub content: Option<String>,
    /// 图片URL
    pub image: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Contact {
    /// 类型(好友/群)
    #[serde(rename = "type")]
    pub type_: String,
    /// 被推荐的好友/群ID
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Location {
    /// 纬度
    pub lat: f64,
    /// 经度
    pub lon: f64,
    /// 标题
    pub title: Option<String>,
    /// 内容
    pub content: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Music {
    /// 类型(qq|163|xm|custom)
    #[serde(rename = "type")]
    pub type_: String,
    /// 歌曲ID
    pub id: Option<String>,
    /// 点击后跳转目标URL
    pub url: Option<String>,
    /// 音乐URL
    pub audio: Option<String>,
    /// 标题
    pub title: Option<String>,
    /// 内容
    pub content: Option<String>,
    /// 图片
    pub image: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Reply {
    /// 回复时引用的消息ID
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Forward {
    /// 合并转发ID
    pub id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Node {
    /// 转发的消息ID
    pub id: Option<String>,
    /// 发送者ID
    pub user_id: Option<String>,
    /// 发送者昵称
    pub nickname: Option<String>,
    /// 消息内容
    pub content: Option<Vec<Segment>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Xml {
    /// XML内容
    pub data: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Json {
    /// JSON内容
    pub data: String,
}

impl std::fmt::Display for Segment {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Segment::Text(seg) => f.write_str(&seg.text),
            Segment::Face(seg) => {
                f.write_str("/[Face")?;
                f.write_str(&seg.id)?;
                f.write_str("]")
            }
            Segment::MarketFace(_) => f.write_str("[表情]"),
            Segment::Image(_) => f.write_str("[图片]"),
            Segment::Record(_) => f.write_str("[语音]"),
            Segment::Video(_) => f.write_str("[视频]"),
            Segment::File(_) => f.write_str("[文件]"),
            Segment::At(seg) => {
                f.write_str("@")?;
                f.write_str(&seg.id)
            }
            Segment::Rps => f.write_str("[猜拳]"),
            Segment::Dice => f.write_str("[掷骰子]"),
            Segment::Shake => f.write_str("[窗口抖动]"),
            Segment::Poke(_) => f.write_str("[戳一戳]"),
            Segment::Anonymous => f.write_str("[匿名]"),
            Segment::Share(share) => {
                f.write_str("[")?;
                f.write_str(&share.title)?;
                f.write_str(",")?;
                f.write_str(&share.url)?;
                f.write_str("]")
            }
            Segment::Contact(_) => f.write_str("[推荐]"),
            Segment::Location(_) => f.write_str("[位置]"),
            Segment::Music(_) => f.write_str("[音乐]"),
            Segment::Reply(_) => f.write_str("[回复]"),
            Segment::Forward(_) => f.write_str("[合并转发]"),
            Segment::Node(_) => f.write_str("[合并转发节点]"),
            Segment::Xml(_) => f.write_str("[XML]"),
            Segment::Json(_) => f.write_str("[JSON]"),
        }
    }
}

macro_rules! segment_builder {
    ($fn_name: ident, $segment_type: tt) => {
        pub fn $fn_name() -> Segment {
            Segment::$segment_type
        }
    };
    ($fn_name: ident, $segment_type: tt, $param: ident: $param_ty: ty) => {
        pub fn $fn_name($param: $param_ty) -> $segment_type {
            $segment_type { $param }
        }
    };
    ($fn_name: ident, $segment_type: tt, $($param: ident: $param_ty: ty),*) => {
        pub fn $fn_name($($param: $param_ty,)*) -> $segment_type {
            $segment_type { $($param,)* }
        }
    };
}

#[allow(dead_code)]
impl Segment {
    segment_builder!(text, Text, text: String);
    segment_builder!(face, Face, id: String);
    segment_builder!(mface, MarketFace, emoji_id: String, url: Option<String>);
    segment_builder!(
        image,
        Image,
        file: String,
        name: Option<String>,
        url: Option<String>,
        summary: Option<String>,
        emoji_id: Option<String>
    );
    segment_builder!(record, Record, file: String, name: Option<String>);
    segment_builder!(video, Video, file: String, name: Option<String>, url: Option<String>);
    segment_builder!(file, File, file: String, name: Option<String>);
    segment_builder!(at, At, id: String);
    segment_builder!(rps, Rps);
    segment_builder!(dice, Dice);
    segment_builder!(shake, Shake);
    segment_builder!(poke, Poke, type_: String, id: String, name: Option<String>);
    segment_builder!(anonymous, Anonymous);
    segment_builder!(
        share,
        Share,
        url: String,
        title: String,
        content: Option<String>,
        image: Option<String>
    );
    segment_builder!(contact, Contact, type_: String, id: String);
    segment_builder!(
        location,
        Location,
        lat: f64,
        lon: f64,
        title: Option<String>,
        content: Option<String>
    );
    segment_builder!(
        music,
        Music,
        type_: String,
        id: Option<String>,
        url: Option<String>,
        audio: Option<String>,
        title: Option<String>,
        content: Option<String>,
        image: Option<String>
    );
    segment_builder!(reply, Reply, id: String);
    segment_builder!(forward, Forward, id: String);
    segment_builder!(
        node,
        Node,
        id: Option<String>,
        user_id: Option<String>,
        nickname: Option<String>,
        content: Option<Vec<Segment>>
    );
    segment_builder!(xml, Xml, data: String);
    segment_builder!(json, Json, data: String);
}
