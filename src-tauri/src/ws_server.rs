use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

type PeerMap = Arc<Mutex<HashMap<Uuid, mpsc::UnboundedSender<Message>>>>;

/// 向所有已连接 Peer 广播消息（排除发送者自己）
async fn broadcast(peers: &PeerMap, msg: Message, exclude: Option<Uuid>) {
    let map = peers.lock().await;
    for (id, tx) in map.iter() {
        if Some(*id) == exclude {
            continue;
        }
        let _ = tx.send(msg.clone());
    }
}

/// 处理单个 WebSocket 连接
async fn handle_connection(
    peers: PeerMap,
    raw_stream: TcpStream,
    _addr: SocketAddr,
    app_tx: mpsc::UnboundedSender<String>,
) {
    let ws_stream = match accept_async(raw_stream).await {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("[ws_server] handshake failed: {e}");
            return;
        }
    };

    let peer_id = Uuid::new_v4();
    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel::<Message>();

    peers.lock().await.insert(peer_id, peer_tx);

    let (mut ws_sink, mut ws_stream) = ws_stream.split();

    // Task: forward messages from peer_rx to WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(msg) = peer_rx.recv().await {
            if ws_sink.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Task: receive from WebSocket, relay to app and broadcast to other peers
    while let Some(msg_result) = ws_stream.next().await {
        match msg_result {
            Ok(msg) if msg.is_text() => {
                let text = msg.to_text().unwrap_or("").to_string();
                // 广播给其他 peers
                broadcast(&peers, Message::Text(text.clone().into()), Some(peer_id)).await;
                // 通知 app 层（写入本地剪切板）
                let _ = app_tx.send(text);
            }
            Ok(msg) if msg.is_close() => break,
            Err(e) => {
                eprintln!("[ws_server] recv error: {e}");
                break;
            }
            _ => {}
        }
    }

    send_task.abort();
    peers.lock().await.remove(&peer_id);
}

pub struct WsServer {
    peers: PeerMap,
    shutdown_tx: Option<mpsc::Sender<()>>,
    port: tokio::sync::Mutex<Option<u16>>,
}

impl WsServer {
    pub fn new() -> Self {
        Self {
            peers: Arc::new(Mutex::new(HashMap::new())),
            shutdown_tx: None,
            port: tokio::sync::Mutex::new(None),
        }
    }

    /// 启动 WebSocket 服务，返回实际绑定端口
    pub async fn start(
        &mut self,
        port: u16,
        app_tx: mpsc::UnboundedSender<String>,
    ) -> Result<u16, String> {
        if self.shutdown_tx.is_some() {
            return Err("服务已在运行中".to_string());
        }

        let addr = format!("0.0.0.0:{port}");
        let listener = TcpListener::bind(&addr)
            .await
            .map_err(|e| format!("绑定端口 {port} 失败: {e}"))?;

        let actual_port = listener
            .local_addr()
            .map(|a| a.port())
            .unwrap_or(port);

        // 保存端口号
        *self.port.lock().await = Some(actual_port);

        let peers = Arc::clone(&self.peers);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        self.shutdown_tx = Some(shutdown_tx);

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, addr)) => {
                                let peers = Arc::clone(&peers);
                                let app_tx = app_tx.clone();
                                tokio::spawn(handle_connection(peers, stream, addr, app_tx));
                            }
                            Err(e) => eprintln!("[ws_server] accept error: {e}"),
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        break;
                    }
                }
            }
        });

        Ok(actual_port)
    }

    /// 停止服务
    pub async fn stop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        self.peers.lock().await.clear();
        *self.port.lock().await = None;
    }

    /// 向所有 Client 广播剪切板消息
    pub async fn broadcast_text(&self, json: String) {
        broadcast(&self.peers, Message::Text(json.into()), None).await;
    }

    /// 当前连接数
    pub async fn peer_count(&self) -> usize {
        self.peers.lock().await.len()
    }

    /// 服务是否正在运行
    pub fn is_running(&self) -> bool {
        self.shutdown_tx.is_some()
    }

    /// 获取服务器端口
    pub async fn get_port(&self) -> u16 {
        self.port.lock().await.unwrap_or(9521)
    }
}

impl Default for WsServer {
    fn default() -> Self {
        Self::new()
    }
}
