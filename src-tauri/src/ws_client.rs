use futures_util::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

pub struct WsClient {
    shutdown_tx: Option<mpsc::Sender<()>>,
    send_tx: Option<mpsc::UnboundedSender<String>>,
    connected_url: tokio::sync::Mutex<Option<String>>,
    /// 是否启用自动重连（使用 Arc 以便在任务间共享）
    auto_reconnect: Arc<AtomicBool>,
}

impl WsClient {
    pub fn new() -> Self {
        Self {
            shutdown_tx: None,
            send_tx: None,
            connected_url: tokio::sync::Mutex::new(None),
            auto_reconnect: Arc::new(AtomicBool::new(true)),
        }
    }

    /// 启用/禁用自动重连
    pub fn set_auto_reconnect(&self, enabled: bool) {
        self.auto_reconnect.store(enabled, Ordering::SeqCst);
    }

    /// 获取自动重连是否启用
    pub fn is_auto_reconnect_enabled(&self) -> bool {
        self.auto_reconnect.load(Ordering::SeqCst)
    }

    /// 连接到 WebSocket Server
    /// - `url`: 形如 `ws://192.168.1.5:9521`
    /// - `app_tx`: 收到远端消息时，通过此 channel 通知 app 层
    pub async fn connect(
        &mut self,
        url: &str,
        app_tx: mpsc::UnboundedSender<String>,
    ) -> Result<(), String> {
        if self.shutdown_tx.is_some() {
            return Err("已有连接，请先断开".to_string());
        }

        // 启用自动重连
        self.auto_reconnect.store(true, Ordering::SeqCst);

        let (ws_stream, _) = connect_async(url)
            .await
            .map_err(|e| format!("连接失败: {e}"))?;

        let (mut ws_sink, mut ws_source) = ws_stream.split();
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);
        let (send_tx, mut send_rx) = mpsc::unbounded_channel::<String>();

        self.shutdown_tx = Some(shutdown_tx);
        self.send_tx = Some(send_tx);
        *self.connected_url.lock().await = Some(url.to_string());

        // 克隆 Arc 供任务使用
        let auto_reconnect = Arc::clone(&self.auto_reconnect);
        let app_tx_clone = app_tx.clone();

        // Task: send outgoing messages to WebSocket
        tokio::spawn(async move {
            while let Some(text) = send_rx.recv().await {
                if ws_sink.send(Message::Text(text.into())).await.is_err() {
                    break;
                }
            }
        });

        // Task: receive from WebSocket, forward to app
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = ws_source.next() => {
                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                let _ = app_tx.send(text.to_string());
                            }
                            Some(Ok(Message::Close(_))) | None => break,
                            Some(Err(e)) => {
                                eprintln!("[ws_client] recv error: {e}");
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ = shutdown_rx.recv() => break,
                }
            }
            // 通知连接断开
            let _ = app_tx_clone.send("__disconnected__".to_string());

            // 如果启用了自动重连，通知需要重连
            if auto_reconnect.load(Ordering::SeqCst) {
                let _ = app_tx_clone.send("__reconnect_needed__".to_string());
            }
        });

        Ok(())
    }

    /// 断开连接
    pub async fn disconnect(&mut self) {
        // 禁用自动重连
        self.auto_reconnect.store(false, Ordering::SeqCst);

        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(()).await;
        }
        self.send_tx = None;
        *self.connected_url.lock().await = None;
    }

    /// 发送消息到 Server（Server 会转发给其他所有 client）
    pub fn send_text(&self, json: String) -> Result<(), String> {
        match &self.send_tx {
            Some(tx) => tx.send(json).map_err(|e| format!("发送失败: {e}")),
            None => Err("未连接".to_string()),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.shutdown_tx.is_some()
    }

    /// 获取当前连接的 URL
    pub async fn get_connected_url(&self) -> Option<String> {
        self.connected_url.lock().await.clone()
    }
}

impl Default for WsClient {
    fn default() -> Self {
        Self::new()
    }
}
