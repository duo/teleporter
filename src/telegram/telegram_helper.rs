use std::{io, slice};

use anyhow::Result;
use flate2::bufread::GzDecoder;
use grammers_client::{
    session::PackedType,
    types::{
        Chat, Message,
        media::{Document, Venue},
    },
};
use grammers_tl_types as tl;
use grammers_tl_types::enums::MessageEntity;
use rgb::{RGBA8, alt::BGRA8};
use rlottie::{Animation, Size, Surface};
use tempfile::NamedTempFile;
use tokio::process::Command;

use super::bridge::Bridge;

type Rgba = rgb::RGBA<u8, bool>;

const GIF_FPS: f64 = 15.0;
const GIF_SIZE: usize = 256;

macro_rules! auto_vectorize {
	(
		pub(crate) fn $ident:ident($($arg_ident:ident : $arg_ty:ty),*) $(-> $ret:ty)? {
			$($body:tt)*
		}
	) => {
		pub(crate) fn $ident($($arg_ident: $arg_ty),*) $(-> $ret)? {
			#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
			#[target_feature(enable = "avx2")]
			#[target_feature(enable = "bmi1")]
			#[target_feature(enable = "bmi2")]
			#[allow(unused_unsafe)]
			unsafe fn avx2($($arg_ident: $arg_ty),*) $(-> $ret)? {
				$($body)*
			}

			#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
			#[target_feature(enable = "sse4.1")]
			#[allow(unused_unsafe)]
			unsafe fn sse4_1($($arg_ident: $arg_ty),*) $(-> $ret)? {
				$($body)*
			}

			fn fallback($($arg_ident: $arg_ty),*) $(-> $ret)? {
				$($body)*
			}

			#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
			if is_x86_feature_detected!("avx2") {
				return unsafe { avx2($($arg_ident),*) };
			}

			#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
			if is_x86_feature_detected!("sse4.1") {
				return unsafe { sse4_1($($arg_ident),*) };
			}

			fallback($($arg_ident),*)
		}
	};
}

pub fn get_command(message: &Message) -> Option<String> {
    if let Some(entities) = message.fmt_entities() {
        if let Some(MessageEntity::BotCommand(cmd)) = entities.first() {
            return Some(
                message.text()[(cmd.offset as usize)..(cmd.offset + cmd.length) as usize]
                    .to_string(),
            );
        }
    }
    None
}

pub fn check_sender(bridge: &Bridge, message: &Message) -> bool {
    // 非Bot发送的消息
    if !message.outgoing() {
        // 发送者是配置的admin id
        if message
            .sender()
            .filter(|c| c.id() == bridge.admin_id)
            .is_some()
        {
            return true;
        }
    }
    false
}

pub fn get_packed_type(message: &Message) -> PackedType {
    match message.chat() {
        Chat::User(_) => PackedType::User,
        Chat::Group(group) => match group.raw {
            grammers_tl_types::enums::Chat::Chat(_) => PackedType::Chat,
            grammers_tl_types::enums::Chat::Channel(channel) => {
                if channel.megagroup {
                    PackedType::Megagroup
                } else if channel.gigagroup {
                    PackedType::Gigagroup
                } else {
                    PackedType::Broadcast
                }
            }
            _ => PackedType::Chat,
        },
        Chat::Channel(_) => PackedType::Broadcast,
    }
}

pub fn get_topic_id(message: &Message) -> Option<i32> {
    match message.reply_header() {
        Some(tl::enums::MessageReplyHeader::Header(header)) => {
            if header.forum_topic && header.reply_to_top_id.is_some() {
                header.reply_to_top_id
            } else {
                header.reply_to_msg_id
            }
        }
        _ => None,
    }
}

pub fn is_raw_photo(document: &Document) -> bool {
    match document.raw.document.as_ref() {
        Some(tl::enums::Document::Document(d)) => {
            for attr in &d.attributes {
                if let tl::enums::DocumentAttribute::ImageSize(_) = attr {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

pub fn is_gif(document: &Document) -> bool {
    if document.raw.video {
        return false;
    };
    match document.raw.document.as_ref() {
        Some(tl::enums::Document::Document(d)) => {
            for attr in &d.attributes {
                if let tl::enums::DocumentAttribute::Video(_) = attr {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

pub fn get_geo(venue: &Venue) -> Option<(f64, f64)> {
    match &venue.raw_venue.geo {
        tl::enums::GeoPoint::Empty => None,
        tl::enums::GeoPoint::Point(geo_point) => Some((geo_point.lat, geo_point.long)),
    }
}

pub async fn video_to_gif(input_data: &[u8]) -> Result<Vec<u8>> {
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
            "-vf",
            "fps=15,scale=256:-1:flags=lanczos,split[s0][s1];\
            [s0]palettegen=max_colors=64[p];\
            [s1][p]paletteuse=dither=sierra2_4a",
            "-f",
            "gif",
            "-loop",
            "0",
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

pub async fn webm_to_gif(input_data: &[u8]) -> Result<Vec<u8>> {
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
            "-filter_complex",
            "[0:v]fps=10,scale=256:-1:flags=lanczos,colorkey=0xffffff:0.01:0.0,split[s0][s1];\
            [s0]palettegen[p];[s1][p]paletteuse",
            "-f",
            "gif",
            "-loop",
            "0",
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

pub async fn tgs_to_gif(id: i64, input_data: &[u8]) -> Result<Vec<u8>> {
    // 解压tgs文件还原成lottie json
    let mut decoder = GzDecoder::new(input_data);
    let mut decompressed_data = Vec::new();

    io::copy(&mut decoder, &mut decompressed_data)?;

    match Animation::from_data(decompressed_data, id.to_string(), ".") {
        Some(mut animation) => {
            // 计算帧采样间隔
            let origianl_fps = animation.framerate();
            let frame_step = (origianl_fps / GIF_FPS).round() as usize;
            if frame_step < 1 {
                return Err(anyhow::anyhow!("Original frame rate is too low"));
            }

            // 输出的GIF数据
            let mut gif_data = Vec::new();

            {
                //let size = animation.size();
                let size = Size::new(GIF_SIZE, GIF_SIZE);

                // 创建GIF编码器
                let gif_width = size.width as u16;
                let gif_height = size.height as u16;
                let mut encoder = gif::Encoder::new(&mut gif_data, gif_width, gif_height, &[])?;
                encoder.set_repeat(gif::Repeat::Infinite)?;
                let frame_delay = (100.0 * frame_step as f64 / animation.framerate()) as u16;

                let buffer_len = size.width * size.height;
                let mut surface = Surface::new(size);
                let mut buffer = vec![RGBA8::default(); buffer_len];
                let frame_count = animation.totalframe();
                let bg = Rgba::new_alpha(0, 0, 0, true);

                for frame in (0..frame_count).step_by(frame_step) {
                    // 渲染当前帧
                    animation.render(frame, &mut surface);
                    // 转换
                    argb_to_rgba(bg, surface.data(), &mut buffer);

                    let data = unsafe {
                        slice::from_raw_parts_mut(buffer.as_mut_ptr() as *mut u8, buffer_len * 4)
                    };

                    // 创建GIF帧
                    let mut frame = gif::Frame::from_rgba_speed(gif_width, gif_height, data, 10);
                    frame.delay = frame_delay;
                    if bg.a {
                        frame.dispose = gif::DisposalMethod::Background;
                    }
                    encoder.write_frame(&frame)?;
                }
            }

            Ok(gif_data)
        }
        None => Err(anyhow::anyhow!("Failed to parse tgs file")),
    }
}

auto_vectorize! {
    pub(crate) fn argb_to_rgba(bg: Rgba, buffer_argb: &[BGRA8], buffer_rgba: &mut [RGBA8]) {
        let bg_r = bg.r as u32;
        let bg_g = bg.g as u32;
        let bg_b = bg.b as u32;

        buffer_argb
            .iter()
            .map(|color| (color.r as u32, color.g as u32, color.b as u32, color.a))
            .map(|(mut r, mut g, mut b, mut a)| {
                if a == 0 {
                    r = 0;
                    g = 0;
                    b = 0;
                }

                let a_neg = (255 - a) as u32;
                r += (bg_r * a_neg) / 255;
                g += (bg_g * a_neg) / 255;
                b += (bg_b * a_neg) / 255;

                if !bg.a || a != 0 {
                    a = 255;
                }

                (r, g, b, a)
            })
            .zip(buffer_rgba.iter_mut())
            .for_each(|((r, g, b, a), rgba)| {
                rgba.r = r as u8;
                rgba.g = g as u8;
                rgba.b = b as u8;
                rgba.a = a;
            });
    }
}
