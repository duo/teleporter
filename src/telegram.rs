mod bridge;
mod command;
mod entities;
mod from_onebot;
mod from_telegram;
mod migration;
mod onebot_helper;
mod telegram_helper;
pub mod telegram_pylon;

#[macro_export]
macro_rules! with_id_lock {
    ($id_lock:expr, $id:expr, $block:expr) => {{
        let arc_mutex = $id_lock
            .entry($id)
            .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
            .clone();
        let _guard = arc_mutex.lock().await;
        $block
    }};
}
