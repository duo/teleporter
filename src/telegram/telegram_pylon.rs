use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use dashmap::DashMap;
use grammers_client::session::Session;
use grammers_client::{Client, Config, FixedReconnect, InitParams, InputMessage, Update};
use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use tokio::sync::mpsc;

use crate::common::TelegramConfig;
use crate::onebot::protocol::{OnebotEvent, OnebotRequest};
use crate::telegram::bridge::{Bridge, RemoteIdLock, TgIdLock};
use crate::telegram::telegram_helper as tg_helper;
use crate::with_id_lock;

use super::bridge::RelayBridge;
use super::migration;

const DB_FILE: &str = "porter.db";

const BOT_SESSION: &str = "bot.session";
const RECONNECTION_POLICY: FixedReconnect = FixedReconnect {
    attempts: usize::MAX,
    delay: Duration::from_secs(5),
};

pub struct TelegramPylon {
    admin_id: i64,
    client: Client,
    db: DatabaseConnection,
}

impl TelegramPylon {
    pub async fn new(config: TelegramConfig) -> Result<Self> {
        // 初始化数据库
        let db = Database::connect(format!("sqlite://{}?mode=rwc", DB_FILE)).await?;
        migration::Migrator::up(&db, None).await?;

        let session = Session::load_file_or_create(BOT_SESSION)
            .context("failed to load or create session for telegram bot")?;
        let client = Client::connect(Config {
            session,
            api_id: config.api_id,
            api_hash: config.api_hash,
            params: InitParams {
                catch_up: false,
                reconnection_policy: &RECONNECTION_POLICY,
                proxy_url: config.proxy_url,
                ..Default::default()
            },
        })
        .await
        .context("failed to connect to telegram")?;

        let is_authorized = client
            .is_authorized()
            .await
            .context("failed to check telegram bot authorization state")?;

        if !is_authorized {
            client
                .bot_sign_in(&config.bot_token)
                .await
                .context("failed to sign in telegram bot")?;

            client
                .session()
                .save_to_file(BOT_SESSION)
                .context("failed to save session for telegram bot")?;
        }

        Ok(Self {
            admin_id: config.admin_id,
            client,
            db,
        })
    }

    pub async fn run(
        &self,
        mut event_receiver: mpsc::Receiver<OnebotEvent>,
        api_sender: mpsc::Sender<OnebotRequest>,
    ) {
        tracing::info!("TelegramPylon started");

        // 初始化处理用辅助
        let bridge = Arc::new(Bridge::new(
            self.admin_id,
            self.client.clone(),
            self.db.clone(),
            api_sender,
        ));

        // 接收Onebot的事件进行处理
        let remote_id_lock: Arc<RemoteIdLock> = Arc::new(DashMap::new());
        let remote_id_lock_clone = remote_id_lock.clone();
        let bridge_clone = bridge.clone();
        let event_handle = tokio::spawn(async move {
            while let Some(event) = event_receiver.recv().await {
                let remote_chat_key = (
                    event.endpoint.clone(),
                    event.raw.get_chat_type(),
                    event.raw.get_chat_id(),
                );
                let id_lock = remote_id_lock.clone();
                let bridge = bridge_clone.clone();
                tokio::spawn(async move {
                    with_id_lock!(id_lock, remote_chat_key, {
                        if let Err(e) = Self::handle_event(&bridge, event).await {
                            tracing::warn!("Failed to handle Onebot event: {}", e);
                        }
                    });
                });
            }
        });

        // 接收Telegram的消息进行处理
        let tg_id_lock: Arc<TgIdLock> = Arc::new(DashMap::new());
        let bridge_clone = bridge.clone();
        let message_handle = tokio::spawn(async move {
            loop {
                let bridge = bridge_clone.clone();
                if let Err(e) =
                    Self::handle_message(tg_id_lock.clone(), remote_id_lock_clone.clone(), bridge)
                        .await
                {
                    tracing::warn!("Failed to handle Telegram message: {}", e);
                }
            }
        });

        let _ = tokio::try_join!(event_handle, message_handle);
    }

    async fn handle_message(
        tg_id_lock: Arc<TgIdLock>,
        remote_id_lock: Arc<RemoteIdLock>,
        bridge: RelayBridge,
    ) -> Result<()> {
        match bridge.bot_client.next_update().await? {
            Update::NewMessage(message) => {
                tracing::info!("Receive Telegram new message: {:?}", message);

                tokio::spawn(async move {
                    with_id_lock!(tg_id_lock, message.chat().id(), {
                        match tg_helper::get_command(&message) {
                            Some(command) => {
                                if let Err(e) =
                                    Self::process_command(&bridge, &message, &command).await
                                {
                                    tracing::warn!("Failed to process Telegram command: {}", e);
                                    let _ = message
                                        .reply(InputMessage::html(
                                            "<b>[WARN] Failed to process command</b>",
                                        ))
                                        .await;
                                }
                            }
                            None => {
                                if let Err(e) =
                                    Self::process_message(&bridge, &message, remote_id_lock).await
                                {
                                    tracing::warn!("Failed to process Telegram message: {}", e);
                                    let _ = message
                                        .reply(InputMessage::html(
                                            "<b>[WARN] Failed to process message</b>",
                                        ))
                                        .await;
                                }
                            }
                        }
                    });
                });
            }
            Update::CallbackQuery(callback) => {
                tracing::debug!("Receive Telegram callback: {:?}", callback);

                tokio::spawn(async move {
                    with_id_lock!(tg_id_lock, callback.chat().id(), {
                        if let Err(e) = Self::process_callback(&bridge, &callback).await {
                            tracing::warn!("Failed to process Telegram callback: {}", e);
                        }
                    });
                });
            }
            _ => {}
        }

        Ok(())
    }
}
