use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use serde_json;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tokio::time::Duration;
use tokio_tungstenite::tungstenite::handshake::server::ErrorResponse;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::{WebSocketStream, tungstenite};

use super::protocol::payload::Payload;
use super::protocol::request::Request;
use super::protocol::response::Response;
use super::protocol::{OnebotEvent, OnebotRequest};
use crate::common::{Endpoint, OnebotConfig, Platform};
use crate::onebot::protocol::event::{Event, LifecycleEvent, MetaEvent};

type EndpointsSenderChannal = Arc<Mutex<HashMap<Endpoint, mpsc::Sender<Arc<Request>>>>>;
type ResponsePendingChannal = Arc<Mutex<HashMap<String, oneshot::Sender<Result<Arc<Response>>>>>>;

// 通道的缓冲区大小
const BUFFER_SIZE: usize = 1024;
// API调用超时时间
const API_TIMOUT: u64 = 120;
// WebSocket读取缓冲区大小
const WS_READ_BUFFER_SIZE: usize = 8 * 1024 * 1024;
// WebSocket最大消息大小
const WS_MAX_MESSAGE_SIZE: usize = 512 * 1024 * 1024;
// WebSocket最大帧大小
const WS_MAX_FRAME_SIZE: usize = 256 * 1024 * 1024;

#[derive(Clone)]
pub struct OnebotPylon {
    // 监听地址
    addr: String,
    // 鉴权
    bearer: Option<String>,
    // 往各端点的请求发送
    endpoints_sender: EndpointsSenderChannal,
    // 待返回的API响应
    response_pending: ResponsePendingChannal,
}

impl OnebotPylon {
    pub async fn new(config: OnebotConfig) -> Result<Self> {
        Ok(Self {
            addr: config.addr,
            bearer: config.token.map(|token| format!("Bearer {}", token)),
            endpoints_sender: Arc::new(Mutex::new(HashMap::new())),
            response_pending: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn run(
        &self,
        event_sender: mpsc::Sender<OnebotEvent>,
        mut api_receiver: mpsc::Receiver<OnebotRequest>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        let try_socket = TcpListener::bind(&self.addr).await;
        let listener = try_socket.expect("Failed to bind");
        tracing::info!("OnebotPylon listening on: {}", self.addr);

        // 将收到的API请求转发给对应端点
        let endpoints_sender = self.endpoints_sender.clone();
        let pending = self.response_pending.clone();
        let mut api_shutdown_rx = shutdown_rx.resubscribe();
        let api_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(req) = api_receiver.recv() => {
                        if let Some(sender) = endpoints_sender.lock().await.get(&req.endpoint) {
                            let echo = req.raw.get_echo();
                            pending.lock().await.insert(echo.clone(), req.ret);
                            if let Err(e) = sender.send(req.raw).await {
                                tracing::warn!("Failed to send request: {}", e);
                                if let Err(e) = pending
                                    .lock()
                                    .await
                                    .remove(echo.as_str())
                                    .unwrap()
                                    .send(Err(e.into()))
                                {
                                    tracing::warn!("Failed to send response: {:?}", e);
                                }
                            }
                        } else if let Err(e) = req
                            .ret
                            .send(Err(anyhow::anyhow!("Client({}) not found", req.endpoint)))
                        {
                            tracing::warn!("Failed to send response: {:?}", e);
                        }
                    }
                    Ok(_) = api_shutdown_rx.recv() => {
                        tracing::info!("Shutting down OnebotPylon API handler");
                        break;
                    }
                }
            }
        });

        let this = self.clone();
        let accept_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, _)) => {
                                let event_sender_clone = event_sender.clone();
                                let onebot_pylon = this.clone();
                                tokio::spawn(async move {
                                    onebot_pylon
                                        .accept_connection(stream, event_sender_clone)
                                        .await;
                                });
                            }
                            Err(e) => {
                                tracing::warn!("Failed to accept connection: {}", e);
                            }
                        }
                    }
                    Ok(_) = shutdown_rx.recv() => {
                        tracing::info!("Shutting down OnebotPylon connection acceptor");
                        break;
                    }
                }
            }
        });

        let _ = tokio::try_join!(api_handle, accept_handle);
        tracing::info!("OnebotPylon shutdown complete");
    }

    pub async fn call_api(
        api_sender: mpsc::Sender<OnebotRequest>,
        endpoint: Endpoint,
        request: Request,
    ) -> Result<Arc<Response>> {
        let (ret, rx) = oneshot::channel();

        let req = OnebotRequest {
            endpoint,
            raw: Arc::new(request),
            ret,
        };
        if let Err(e) = api_sender.send(req).await {
            return Err(anyhow::anyhow!("Failed to send request: {}", e));
        }

        match tokio::time::timeout(Duration::from_secs(API_TIMOUT), rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(e)) => Err(e.into()),
            Err(e) => Err(e.into()),
        }
    }

    async fn accept_connection(&self, stream: TcpStream, event_sender: mpsc::Sender<OnebotEvent>) {
        let addr = stream
            .peer_addr()
            .expect("connected streams should have a peer address");

        let endpoint_locked = Arc::new(std::sync::Mutex::new(Endpoint::default()));
        let callback =
            |req: &tungstenite::handshake::server::Request,
             mut response: tungstenite::handshake::server::Response| {
                let auth_header = req
                    .headers()
                    .get("Authorization")
                    .and_then(|h| h.to_str().ok())
                    .map(|h| h.to_string());

                // 检查请求头中的Authorization
                if auth_header != self.bearer {
                    *response.status_mut() = tungstenite::http::StatusCode::UNAUTHORIZED;
                    return Err(ErrorResponse::default());
                }

                let x_self_id = req.headers().get("X-Self-ID").and_then(|h| h.to_str().ok());
                let user_agent = req
                    .headers()
                    .get("User-Agent")
                    .and_then(|h| h.to_str().ok());

                // 检查请求头中的X-Self-ID和User-Agent
                if x_self_id.is_none() || user_agent.is_none() {
                    *response.status_mut() = tungstenite::http::StatusCode::BAD_REQUEST;
                    return Err(ErrorResponse::default());
                }

                let platform = match user_agent.unwrap() {
                    ua if ua.starts_with("LLOneBot") => Platform::QQ,
                    ua if ua.starts_with("WeChat") => Platform::WeChat,
                    _ => Platform::QQ,
                };

                *(endpoint_locked.lock().unwrap()) = Endpoint {
                    platform,
                    id: x_self_id.unwrap().to_string(),
                };

                Ok(response)
            };
        let mut config = WebSocketConfig::default();
        config.read_buffer_size = WS_READ_BUFFER_SIZE;
        config.max_message_size = Some(WS_MAX_MESSAGE_SIZE);
        config.max_frame_size = Some(WS_MAX_FRAME_SIZE);

        let ws_stream: WebSocketStream<TcpStream> =
            tokio_tungstenite::accept_hdr_async_with_config(stream, callback, Some(config))
                .await
                .expect("Error during the websocket handshake occurred");

        // 通过回调后获得端点
        let endpoint = endpoint_locked.lock().unwrap().clone();

        tracing::info!("New Onebot client ({}) connection: {}", endpoint, addr);

        let (mut write, mut read) = ws_stream.split();

        // 接收API请求
        let (sender, mut receiver) = mpsc::channel(BUFFER_SIZE);
        self.endpoints_sender
            .lock()
            .await
            .insert(endpoint.clone(), sender);
        tokio::spawn(async move {
            while let Some(req) = receiver.recv().await {
                Self::handle_request(req, &mut write).await;
            }
        });

        // 接收WebSocket消息
        let sender = event_sender.clone();
        let endpoints_sender = self.endpoints_sender.clone();
        let pending = self.response_pending.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(message) => {
                        Self::handle_message(&endpoint, &message, &sender, &pending).await;
                    }
                    Err(e) => {
                        // 发送断开事件
                        let event = Event::Meta(MetaEvent::Lifecycle(LifecycleEvent {
                            time: Utc::now().timestamp(),
                            self_id: endpoint.id.clone(),
                            sub_type: "disconnect".to_string(),
                        }));
                        if let Err(e) = sender
                            .send(OnebotEvent {
                                endpoint: endpoint.clone(),
                                raw: Arc::new(event),
                            })
                            .await
                        {
                            tracing::warn!("Failed to send event: {}", e);
                        }

                        endpoints_sender.lock().await.remove(&endpoint);
                        tracing::warn!("Onebot client ({}) connection error: {}", endpoint, e);
                        break;
                    }
                }
            }
        });
    }

    async fn handle_message(
        endpoint: &Endpoint,
        msg: &tungstenite::Message,
        sender: &mpsc::Sender<OnebotEvent>,
        pending: &ResponsePendingChannal,
    ) {
        if let tungstenite::Message::Text(text) = msg {
            tracing::debug!("Received onebot message: {}", text);
            match serde_json::from_str::<Payload>(text) {
                Ok(payload) => match payload {
                    // 上报Event
                    Payload::Event(event) => {
                        if let Err(e) = sender
                            .send(OnebotEvent {
                                endpoint: endpoint.clone(),
                                raw: event,
                            })
                            .await
                        {
                            tracing::warn!("Failed to send event: {}", e);
                        }
                    }
                    // 返回Response
                    Payload::Response(response) => {
                        if let Some(p) = pending.lock().await.remove(&response.echo) {
                            if let Err(e) = p.send(Ok(response)) {
                                tracing::warn!("Failed to send response: {:?}", e);
                            }
                        }
                    }
                    // 不应该收到Request
                    Payload::Request(request) => {
                        tracing::warn!("Unexpected request: {:?}", request);
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to deserialize message: {}\n{}", e, text);
                }
            }
        }
    }

    async fn handle_request(
        req: Arc<Request>,
        write: &mut (
                 impl SinkExt<
            tungstenite::protocol::Message,
            Error = tokio_tungstenite::tungstenite::Error,
        > + Unpin
             ),
    ) {
        match serde_json::to_string(&*req) {
            Ok(json_string) => {
                if let Err(e) = write
                    .send(tungstenite::Message::Text(json_string.into()))
                    .await
                {
                    tracing::warn!("Failed to send message: {}", e);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to serialize message: {}", e);
            }
        }
    }
}
