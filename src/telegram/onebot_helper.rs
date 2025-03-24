use anyhow::Result;
use grammers_tl_types::enums::InputGeoPoint;
use grammers_tl_types::types::InputMediaVenue;
use image::GenericImageView;
use phf::phf_map;
use serde_json::Value;
use serde_json_path::JsonPath;
use tempfile::NamedTempFile;
use tokio::process::Command;
use webp::Encoder;

use crate::onebot::protocol::segment::Segment;

pub fn is_sticker(segment: &Segment) -> bool {
    match segment {
        Segment::MarketFace(_) => true,
        Segment::Image(image) => match image.emoji_id {
            Some(_) => true,
            None => image
                .summary
                .as_ref()
                .is_some_and(|summary| summary.as_str() == "[动画表情]"),
        },
        _ => false,
    }
}

pub fn image_size(image_data: &[u8], mime_type: &str) -> (u32, u32) {
    if mime_type.starts_with("image") {
        if let Ok(img) = image::load_from_memory(image_data) {
            return img.dimensions();
        }
    }

    (0, 0)
}

pub fn img_to_webp(image_data: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(image_data)?;
    let (width, height) = img.dimensions();
    let quality = 85.0;

    let webp_data = if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        Encoder::from_rgba(rgba.as_raw(), width, height)
            .encode(quality)
            .to_vec()
    } else {
        let rgb = img.to_rgb8();
        Encoder::from_rgb(rgb.as_raw(), width, height)
            .encode(quality)
            .to_vec()
    };

    Ok(webp_data.to_vec())
}

pub async fn gif_to_webm(input_data: &[u8]) -> Result<Vec<u8>> {
    // 创建临时文件 (通过管道作为输入只能顺序访问, 在转换时容易出现问题)
    let temp_file = NamedTempFile::new()?;
    let input_path = temp_file
        .path()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid temp path"))?;

    // 将输入数据写入临时文件
    tokio::fs::write(input_path, input_data).await?;

    let child = Command::new("ffmpeg")
        .args([
            "-i",
            input_path,
            "-r",
            "30",
            "-t",
            "2.99",
            "-an",
            "-c:v",
            "libvpx-vp9",
            "-pix_fmt",
            "yuva420p",
            "-vf",
            "''scale=512:512:force_original_aspect_ratio=decrease'",
            "-b:v",
            "400K",
            "-f",
            "webm",
            "pipe:1",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("ffmpeg exited: {}", output.status));
    }

    Ok(output.stdout)
}

pub async fn wav_to_ogg(input_data: &[u8]) -> Result<Vec<u8>> {
    // 创建临时文件 (通过管道作为输入只能顺序访问, 在转换时容易出现问题)
    let temp_file = NamedTempFile::new()?;
    let input_path = temp_file
        .path()
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid temp path"))?;

    // 将输入数据写入临时文件
    tokio::fs::write(input_path, input_data).await?;

    let child = Command::new("ffmpeg")
        .args([
            "-i", input_path, "-c:a", "libopus", "-b:a", "24K", "-f", "ogg", "pipe:1",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("ffmpeg exited: {}", output.status));
    }

    Ok(output.stdout)
}

pub fn extract_location_from_json(json: &Value) -> Result<InputMediaVenue> {
    let title = JsonPath::parse("$.meta.*.name")?
        .query(json)
        .exactly_one()
        .map(|v| v.as_str().unwrap())?
        .to_string();
    let address = JsonPath::parse("$.meta.*.address")?
        .query(json)
        .exactly_one()
        .map(|v| v.as_str().unwrap())?
        .to_string();
    let lat = JsonPath::parse("$.meta.*.lat")?
        .query(json)
        .exactly_one()
        .map(|v| v.as_str().unwrap().parse::<f64>().unwrap())?;
    let long = JsonPath::parse("$.meta.*.lng")?
        .query(json)
        .exactly_one()
        .map(|v| v.as_str().unwrap().parse::<f64>().unwrap())?;

    Ok(InputMediaVenue {
        geo_point: InputGeoPoint::Point(grammers_tl_types::types::InputGeoPoint {
            lat,
            long,
            accuracy_radius: None,
        }),
        title,
        address,
        provider: String::new(),
        venue_id: String::new(),
        venue_type: String::new(),
    })
}

pub fn extract_share_from_json(json: &Value) -> Result<String> {
    let (title, description, source, url);

    let doc_node = JsonPath::parse("$.meta.*.qqdocurl")?.query(json);
    if doc_node.is_empty() {
        let jump_node = JsonPath::parse("$.meta.*.jumpUrl")?.query(json);
        if jump_node.is_empty() {
            return Ok(String::new());
        } else {
            url = jump_node.exactly_one().map(|v| v.as_str().unwrap())?;
            source = JsonPath::parse("$.meta.*.tag")?
                .query(json)
                .exactly_one()
                .map(|v| v.as_str().unwrap())?;
            description = JsonPath::parse("$.meta.*.desc")?
                .query(json)
                .exactly_one()
                .map(|v| v.as_str().unwrap())?;
            title = JsonPath::parse("$.prompt")?
                .query(json)
                .exactly_one()
                .map(|v| v.as_str().unwrap())?;
        }
    } else {
        url = doc_node.exactly_one().map(|v| v.as_str().unwrap())?;
        source = JsonPath::parse("$.meta.*.title")?
            .query(json)
            .exactly_one()
            .map(|v| v.as_str().unwrap())?;
        description = JsonPath::parse("$.meta.*.desc")?
            .query(json)
            .exactly_one()
            .map(|v| v.as_str().unwrap())?;
        title = JsonPath::parse("$.prompt")?
            .query(json)
            .exactly_one()
            .map(|v| v.as_str().unwrap())?;
    }

    Ok(format!(
        "<u>{}</u>\n\n{}\n\nvia <a href=\"{}\">{}</a>",
        html_escape::encode_text(title),
        html_escape::encode_text(description),
        html_escape::encode_text(url),
        html_escape::encode_text(source),
    ))
}

static QQ_EMOJI: phf::Map<&'static str, &'static str> = phf_map! {
    "0" => "惊讶",
    "1" => "撇嘴",
    "2" => "色",
    "3" => "发呆",
    "4" => "得意",
    "5" => "流泪",
    "6" => "害羞",
    "7" => "闭嘴",
    "8" => "睡",
    "9" => "大哭",
    "10" => "尴尬",
    "11" => "发怒",
    "12" => "调皮",
    "13" => "呲牙",
    "14" => "微笑",
    "15" => "难过",
    "16" => "酷",
    "18" => "抓狂",
    "19" => "吐",
    "20" => "偷笑",
    "21" => "可爱",
    "22" => "白眼",
    "23" => "傲慢",
    "24" => "饥饿",
    "25" => "困",
    "26" => "惊恐",
    "27" => "流汗",
    "28" => "憨笑",
    "29" => "悠闲",
    "30" => "奋斗",
    "31" => "咒骂",
    "32" => "疑问",
    "33" => "嘘",
    "34" => "晕",
    "35" => "折磨",
    "36" => "衰",
    "37" => "骷髅",
    "38" => "敲打",
    "39" => "再见",
    "41" => "发抖",
    "42" => "爱情",
    "43" => "跳跳",
    "46" => "猪头",
    "49" => "拥抱",
    "53" => "蛋糕",
    "54" => "闪电",
    "55" => "炸弹",
    "56" => "刀",
    "57" => "足球",
    "59" => "便便",
    "60" => "咖啡",
    "61" => "饭",
    "63" => "玫瑰",
    "64" => "凋谢",
    "66" => "爱心",
    "67" => "心碎",
    "69" => "礼物",
    "74" => "太阳",
    "75" => "月亮",
    "76" => "赞",
    "77" => "踩",
    "78" => "握手",
    "79" => "胜利",
    "85" => "飞吻",
    "86" => "怄火",
    "89" => "西瓜",
    "96" => "冷汗",
    "97" => "擦汗",
    "98" => "抠鼻",
    "99" => "鼓掌",
    "100" => "糗大了",
    "101" => "坏笑",
    "102" => "左哼哼",
    "103" => "右哼哼",
    "104" => "哈欠",
    "105" => "鄙视",
    "106" => "委屈",
    "107" => "快哭了",
    "108" => "阴险",
    "109" => "左亲亲",
    "110" => "吓",
    "111" => "可怜",
    "112" => "菜刀",
    "113" => "啤酒",
    "114" => "篮球",
    "115" => "乒乓",
    "116" => "示爱",
    "117" => "瓢虫",
    "118" => "抱拳",
    "119" => "勾引",
    "120" => "拳头",
    "121" => "差劲",
    "122" => "爱你",
    "123" => "NO",
    "124" => "OK",
    "125" => "转圈",
    "126" => "磕头",
    "127" => "回头",
    "128" => "跳绳",
    "129" => "挥手",
    "130" => "激动",
    "131" => "街舞",
    "132" => "献吻",
    "133" => "左太极",
    "134" => "右太极",
    "136" => "双喜",
    "137" => "鞭炮",
    "138" => "灯笼",
    "140" => "K歌",
    "144" => "喝彩",
    "145" => "祈祷",
    "146" => "爆筋",
    "147" => "棒棒糖",
    "148" => "喝奶",
    "151" => "飞机",
    "158" => "钞票",
    "168" => "药",
    "169" => "手枪",
    "171" => "茶",
    "172" => "眨眼睛",
    "173" => "泪奔",
    "174" => "无奈",
    "175" => "卖萌",
    "176" => "小纠结",
    "177" => "喷血",
    "178" => "斜眼笑",
    "179" => "doge",
    "180" => "惊喜",
    "181" => "骚扰",
    "182" => "笑哭",
    "183" => "我最美",
    "184" => "河蟹",
    "185" => "羊驼",
    "187" => "幽灵",
    "188" => "蛋",
    "190" => "菊花",
    "192" => "红包",
    "193" => "大笑",
    "194" => "不开心",
    "197" => "冷漠",
    "198" => "呃",
    "199" => "好棒",
    "200" => "拜托",
    "201" => "点赞",
    "202" => "无聊",
    "203" => "托脸",
    "204" => "吃",
    "205" => "送花",
    "206" => "害怕",
    "207" => "花痴",
    "208" => "小样儿",
    "210" => "飙泪",
    "211" => "我不看",
    "212" => "托腮",
    "214" => "啵啵",
    "215" => "糊脸",
    "216" => "拍头",
    "217" => "扯一扯",
    "218" => "舔一舔",
    "219" => "蹭一蹭",
    "220" => "拽炸天",
    "221" => "顶呱呱",
    "222" => "抱抱",
    "223" => "暴击",
    "224" => "开枪",
    "225" => "撩一撩",
    "226" => "拍桌",
    "227" => "拍手",
    "228" => "恭喜",
    "229" => "干杯",
    "230" => "嘲讽",
    "231" => "哼",
    "232" => "佛系",
    "233" => "掐一掐",
    "234" => "惊呆",
    "235" => "颤抖",
    "236" => "啃头",
    "237" => "偷看",
    "238" => "扇脸",
    "239" => "原谅",
    "240" => "喷脸",
    "241" => "生日快乐",
    "242" => "头撞击",
    "243" => "甩头",
    "244" => "扔狗",
    "245" => "加油必胜",
    "246" => "加油抱抱",
    "247" => "口罩护体",
    "260" => "搬砖中",
    "261" => "忙到飞起",
    "262" => "脑阔疼",
    "263" => "沧桑",
    "264" => "捂脸",
    "265" => "辣眼睛",
    "266" => "哦哟",
    "267" => "头秃",
    "268" => "问号脸",
    "269" => "暗中观察",
    "270" => "emm",
    "271" => "吃瓜",
    "272" => "呵呵哒",
    "273" => "我酸了",
    "274" => "太南了",
    "276" => "辣椒酱",
    "277" => "汪汪",
    "278" => "汗",
    "279" => "打脸",
    "280" => "击掌",
    "281" => "无眼笑",
    "282" => "敬礼",
    "283" => "狂笑",
    "284" => "面无表情",
    "285" => "摸鱼",
    "286" => "魔鬼笑",
    "287" => "哦",
    "288" => "请",
    "289" => "睁眼",
    "290" => "敲开心",
    "291" => "震惊",
    "292" => "让我康康",
    "293" => "摸锦鲤",
    "294" => "期待",
    "295" => "拿到红包",
    "296" => "真好",
    "297" => "拜谢",
    "298" => "元宝",
    "299" => "牛啊",
    "300" => "胖三斤",
    "301" => "好闪",
    "302" => "左拜年",
    "303" => "右拜年",
    "304" => "红包包",
    "305" => "右亲亲",
    "306" => "牛气冲天",
    "307" => "喵喵",
    "308" => "求红包",
    "309" => "谢红包",
    "310" => "新年烟花",
    "311" => "打call",
    "312" => "变形",
    "313" => "嗑到了",
    "314" => "仔细分析",
    "315" => "加油",
    "316" => "我没事",
    "317" => "菜汪",
    "318" => "崇拜",
    "319" => "比心",
    "320" => "庆祝",
    "321" => "老色痞",
    "322" => "拒绝",
    "323" => "嫌弃",
    "324" => "吃糖",
    "325" => "惊吓",
    "326" => "生气",
    "327" => "加一",
    "328" => "错号",
    "329" => "对号",
    "330" => "完成",
    "331" => "明白",
    "332" => "举牌牌",
    "333" => "烟花",
    "334" => "虎虎生威",
    "336" => "豹富",
    "337" => "花朵脸",
    "338" => "我想开了",
    "339" => "舔屏",
    "340" => "热化了",
    "341" => "打招呼",
    "342" => "酸Q",
    "343" => "我方了",
    "344" => "大怨种",
    "345" => "红包多多",
    "346" => "你真棒棒",
    "347" => "大展宏兔",
    "348" => "福萝卜",
};

const WECHAT_EMOJI_REPLACEMENTS: &[(&str, &str)] = &[
    ("[微笑]", "😃"),
    ("[Smile]", "😃"),
    ("[色]", "😍"),
    ("[Drool]", "😍"),
    ("[发呆]", "😳"),
    ("[Scowl]", "😳"),
    ("[得意]", "😎"),
    ("[Chill]", "😎"),
    ("[流泪]", "😭"),
    ("[Sob]", "😭"),
    ("[害羞]", "☺️"),
    ("[Shy]", "☺️"),
    ("[闭嘴]", "🤐"),
    ("[Shutup]", "🤐"),
    ("[睡]", "😴"),
    ("[Sleep]", "😴"),
    ("[大哭]", "😣"),
    ("[Cry]", "😣"),
    ("[尴尬]", "😰"),
    ("[Awkward]", "😰"),
    ("[发怒]", "😡"),
    ("[Pout]", "😡"),
    ("[调皮]", "😜"),
    ("[Wink]", "😜"),
    ("[呲牙]", "😁"),
    ("[Grin]", "😁"),
    ("[惊讶]", "😱"),
    ("[Surprised]", "😱"),
    ("[难过]", "🙁"),
    ("[Frown]", "🙁"),
    ("[囧]", "☺️"),
    ("[Tension]", "☺️"),
    ("[抓狂]", "😫"),
    ("[Scream]", "😫"),
    ("[吐]", "🤢"),
    ("[Puke]", "🤢"),
    ("[偷笑]", "🙈"),
    ("[Chuckle]", "🙈"),
    ("[愉快]", "☺️"),
    ("[Joyful]", "☺️"),
    ("[白眼]", "🙄"),
    ("[Slight]", "🙄"),
    ("[傲慢]", "😕"),
    ("[Smug]", "😕"),
    ("[困]", "😪"),
    ("[Drowsy]", "😪"),
    ("[惊恐]", "😱"),
    ("[Panic]", "😱"),
    ("[流汗]", "😓"),
    ("[Sweat]", "😓"),
    ("[憨笑]", "😄"),
    ("[Laugh]", "😄"),
    ("[悠闲]", "😏"),
    ("[Loafer]", "😏"),
    ("[奋斗]", "💪"),
    ("[Strive]", "💪"),
    ("[咒骂]", "😤"),
    ("[Scold]", "😤"),
    ("[疑问]", "❓"),
    ("[Doubt]", "❓"),
    ("[嘘]", "🤐"),
    ("[Shhh]", "🤐"),
    ("[晕]", "😲"),
    ("[Dizzy]", "😲"),
    ("[衰]", "😳"),
    ("[BadLuck]", "😳"),
    ("[骷髅]", "💀"),
    ("[Skull]", "💀"),
    ("[敲打]", "👊"),
    ("[Hammer]", "👊"),
    ("[再见]", "🙋♂"),
    ("[Bye]", "🙋♂"),
    ("[擦汗]", "😥"),
    ("[Relief]", "😥"),
    ("[抠鼻]", "🤷♂"),
    ("[DigNose]", "🤷♂"),
    ("[鼓掌]", "👏"),
    ("[Clap]", "👏"),
    ("[坏笑]", "👻"),
    ("[Trick]", "👻"),
    ("[左哼哼]", "😾"),
    ("[Bah！L]", "😾"),
    ("[右哼哼]", "😾"),
    ("[Bah！R]", "😾"),
    ("[哈欠]", "😪"),
    ("[Yawn]", "😪"),
    ("[鄙视]", "😒"),
    ("[Lookdown]", "😒"),
    ("[委屈]", "😣"),
    ("[Wronged]", "😣"),
    ("[快哭了]", "😔"),
    ("[Puling]", "😔"),
    ("[阴险]", "😈"),
    ("[Sly]", "😈"),
    ("[亲亲]", "😘"),
    ("[Kiss]", "😘"),
    ("[可怜]", "😻"),
    ("[Whimper]", "😻"),
    ("[菜刀]", "🔪"),
    ("[Cleaver]", "🔪"),
    ("[西瓜]", "🍉"),
    ("[Melon]", "🍉"),
    ("[啤酒]", "🍺"),
    ("[Beer]", "🍺"),
    ("[咖啡]", "☕"),
    ("[Coffee]", "☕"),
    ("[猪头]", "🐷"),
    ("[Pig]", "🐷"),
    ("[玫瑰]", "🌹"),
    ("[Rose]", "🌹"),
    ("[凋谢]", "🥀"),
    ("[Wilt]", "🥀"),
    ("[嘴唇]", "💋"),
    ("[Lip]", "💋"),
    ("[爱心]", "❤️"),
    ("[Heart]", "❤️"),
    ("[心碎]", "💔"),
    ("[BrokenHeart]", "💔"),
    ("[蛋糕]", "🎂"),
    ("[Cake]", "🎂"),
    ("[炸弹]", "💣"),
    ("[Bomb]", "💣"),
    ("[便便]", "💩"),
    ("[Poop]", "💩"),
    ("[月亮]", "🌃"),
    ("[Moon]", "🌃"),
    ("[太阳]", "🌞"),
    ("[Sun]", "🌞"),
    ("[拥抱]", "🤗"),
    ("[Hug]", "🤗"),
    ("[强]", "👍"),
    ("[Strong]", "👍"),
    ("[弱]", "👎"),
    ("[Weak]", "👎"),
    ("[握手]", "🤝"),
    ("[Shake]", "🤝"),
    ("[胜利]", "✌️"),
    ("[Victory]", "✌️"),
    ("[抱拳]", "🙏"),
    ("[Salute]", "🙏"),
    ("[勾引]", "💁♂"),
    ("[Beckon]", "💁♂"),
    ("[拳头]", "👊"),
    ("[Fist]", "👊"),
    ("[OK]", "👌"),
    ("[跳跳]", "💃"),
    ("[Waddle]", "💃"),
    ("[发抖]", "🙇"),
    ("[Tremble]", "🙇"),
    ("[怄火]", "😡"),
    ("[Aaagh!]", "😡"),
    ("[转圈]", "🕺"),
    ("[Twirl]", "🕺"),
    ("[嘿哈]", "🤣"),
    ("[Hey]", "🤣"),
    ("[捂脸]", "🤦♂"),
    ("[Facepalm]", "🤦♂"),
    ("[奸笑]", "😜"),
    ("[Smirk]", "😜"),
    ("[机智]", "🤓"),
    ("[Smart]", "🤓"),
    ("[皱眉]", "😟"),
    ("[Concerned]", "😟"),
    ("[耶]", "✌️"),
    ("[Yeah!]", "✌️"),
    ("[红包]", "🧧"),
    ("[Packet]", "🧧"),
    ("[鸡]", "🐥"),
    ("[Chick]", "🐥"),
    ("[蜡烛]", "🕯️"),
    ("[Candle]", "🕯️"),
    ("[糗大了]", "😥"),
    ("[ThumbsUp]", "👍"),
    ("[ThumbsDown]", "👎"),
    ("[Peace]", "✌️"),
    ("[Pleased]", "😊"),
    ("[Rich]", "🀅"),
    ("[Pup]", "🐶"),
    ("[吃瓜]", "🙄🍉"),
    ("[Onlooker]", "🙄🍉"),
    ("[加油]", "💪😁"),
    ("[GoForIt]", "💪😁"),
    ("[加油加油]", "💪😷"),
    ("[汗]", "😓"),
    ("[Sweats]", "😓"),
    ("[天啊]", "😱"),
    ("[OMG]", "😱"),
    ("[Emm]", "🤔"),
    ("[社会社会]", "😏"),
    ("[Respect]", "😏"),
    ("[旺柴]", "🐶😏"),
    ("[Doge]", "🐶😏"),
    ("[好的]", "😏👌"),
    ("[NoProb]", "😏👌"),
    ("[哇]", "🤩"),
    ("[Wow]", "🤩"),
    ("[打脸]", "😟🤚"),
    ("[MyBad]", "😟🤚"),
    ("[破涕为笑]", "😂"),
    ("[破涕為笑]", "😂"),
    ("[Lol]", "😂"),
    ("[苦涩]", "😭"),
    ("[Hurt]", "😭"),
    ("[翻白眼]", "🙄"),
    ("[Boring]", "🙄"),
    ("[裂开]", "🫠"),
    ("[Broken]", "🫠"),
    ("[爆竹]", "🧨"),
    ("[Firecracker]", "🧨"),
    ("[烟花]", "🎆"),
    ("[Fireworks]", "🎆"),
    ("[福]", "🧧"),
    ("[Blessing]", "🧧"),
    ("[礼物]", "🎁"),
    ("[Gift]", "🎁"),
    ("[庆祝]", "🎉"),
    ("[Party]", "🎉"),
    ("[合十]", "🙏"),
    ("[Worship]", "🙏"),
    ("[叹气]", "😮💨"),
    ("[Sigh]", "😮💨"),
    ("[让我看看]", "👀"),
    ("[LetMeSee]", "👀"),
    ("[666]", "6️⃣6️⃣6️⃣"),
    ("[无语]", "😑"),
    ("[Duh]", "😑"),
    ("[失望]", "😞"),
    ("[Let Down]", "😞"),
    ("[恐惧]", "😨"),
    ("[Terror]", "😨"),
    ("[脸红]", "😳"),
    ("[Flushed]", "😳"),
    ("[生病]", "😷"),
    ("[Sick]", "😷"),
    ("[笑脸]", "😁"),
    ("[Happy]", "😁"),
];

pub fn replace_qq_face(id: &str) -> String {
    QQ_EMOJI
        .get(id)
        .map(|v| format!("/{}", v))
        .unwrap_or_else(|| format!("/[Face{}]", id))
}

pub fn replace_wechat_emoji(content: &str) -> String {
    let mut result = content.to_string();
    for &(old, new) in WECHAT_EMOJI_REPLACEMENTS {
        result = result.replace(old, new);
    }
    result
}
