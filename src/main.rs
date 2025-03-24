mod common;
mod onebot;
mod telegram;

use tokio::sync::mpsc;
use tracing::Level;
use tracing_log::LogTracer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, fmt};

use crate::common::TeleporterConfig;
use crate::onebot::onebot_pylon::OnebotPylon;
use crate::telegram::telegram_pylon::TelegramPylon;

const BUFFER_SIZE: usize = 1024;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let config = TeleporterConfig::load();

    // 设置日志
    LogTracer::init().expect("Failed to set logger");
    let log_level = config
        .general
        .log_level
        .parse::<Level>()
        .unwrap_or(Level::INFO);
    let file_appender = tracing_appender::rolling::daily("logs", "porter.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let subscriber = tracing_subscriber::registry()
        .with(
            EnvFilter::from_default_env()
                .add_directive(log_level.into())
                .add_directive("sqlx::query=off".parse().unwrap()),
        )
        .with(fmt::Layer::new().with_writer(std::io::stdout))
        .with(fmt::Layer::new().with_writer(non_blocking).with_ansi(false));
    tracing::subscriber::set_global_default(subscriber).expect("Unable to set a global subscriber");

    let telegram_pylon = TelegramPylon::new(config.telegram).await.unwrap();
    let onebot_pylon = OnebotPylon::new(config.onebot).await.unwrap();

    let (event_sender, event_receiver) = mpsc::channel(BUFFER_SIZE);
    let (api_sender, api_receiver) = mpsc::channel(BUFFER_SIZE);

    let telegram_handle = tokio::spawn(async move {
        telegram_pylon.run(event_receiver, api_sender).await;
    });

    let onebot_handle = tokio::spawn(async move {
        onebot_pylon.run(event_sender, api_receiver).await;
    });

    let _ = tokio::try_join!(telegram_handle, onebot_handle);
}
