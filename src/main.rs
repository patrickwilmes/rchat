/*
 * SPDX-License-Identifier: CC-BY-NC-ND-4.0
 *
 * rchat
 * Copyright (c) 2024 Patrick Wilmes <p.wilmes89@gmail.com>
 *
 * This file is licensed under the Creative Commons Attribution-NonCommercial-NoDerivatives 4.0 International License.
 * You may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     https://creativecommons.org/licenses/by-nc-nd/4.0/
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use warp::Filter;
use warp::ws::{Message, WebSocket};
use tokio::sync::{mpsc, RwLock};
use tokio::task;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug)]
struct ChatMessage {
    username: String,
    message: String,
}

type Clients = Arc<RwLock<Vec<mpsc::UnboundedSender<Result<Message, warp::Error>>>>>;

#[tokio::main]
async fn main() {
    let clients: Clients = Arc::new(RwLock::new(Vec::new()));

    let chat = warp::path("chat")
        .and(warp::ws())
        .and(with_clients(clients.clone()))
        .map(|ws: warp::ws::Ws, clients| {
            ws.on_upgrade(move |socket| handle_connection(socket, clients))
        });

    warp::serve(chat).run(([127, 0, 0, 1], 3030)).await;
}

fn with_clients(clients: Clients) -> impl Filter<Extract = (Clients,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || clients.clone())
}

async fn handle_connection(ws: WebSocket, clients: Clients) {
    let (mut ws_tx, mut ws_rx) = ws.split();
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();

    {
        let mut clients = clients.write().await;
        clients.push(client_tx);
    }

    let clients_clone = clients.clone();
    let send_task = task::spawn(async move {
        while let Some(result) = client_rx.recv().await {
            if ws_tx.send(result.unwrap()).await.is_err() {
                break;
            }
        }
    });

    while let Some(result) = ws_rx.next().await {
        if let Ok(msg) = result {
            if msg.is_text() {
                let msg_text = msg.to_str().unwrap();
                let chat_msg: ChatMessage = serde_json::from_str(msg_text).unwrap();
                let broadcast_msg = serde_json::to_string(&chat_msg).unwrap();

                let clients = clients_clone.read().await;
                for client in clients.iter() {
                    if let Err(_) = client.send(Ok(Message::text(broadcast_msg.clone()))) {
                        // Handle client send error if necessary
                    }
                }
            }
        }
    }

    {
        let mut clients = clients.write().await;
        clients.retain(|client| !client.is_closed());
    }

    send_task.await.unwrap();
}

