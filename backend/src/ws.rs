use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::Response,
};
use tokio::sync::broadcast::error::RecvError;

use crate::{auth::AuthUser, state::AppState};

pub async fn handler(_user: AuthUser, State(st): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| pump(socket, st))
}

async fn pump(mut socket: WebSocket, st: AppState) {
    let mut rx = st.env.hub.subscribe();
    loop {
        tokio::select! {
            ev = rx.recv() => match ev {
                Ok(text) => { if socket.send(Message::Text(text.into())).await.is_err() { break; } }
                Err(RecvError::Lagged(n)) => {

                    let _ = socket.send(Message::Text(format!(r#"{{"event":"resync","data":{{"missed":{n}}}}}"#).into())).await;
                }
                Err(RecvError::Closed) => break,
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                Some(Ok(Message::Ping(p))) => { let _ = socket.send(Message::Pong(p)).await; }
                _ => {}
            },
        }
    }
}
