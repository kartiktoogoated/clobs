use actix::prelude::*;
use actix_web_actors::ws;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct Broadcaster {
    clients: Arc<Mutex<Vec<Recipient<WsMessage>>>>,
}

impl Broadcaster {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(Vec::new())),
        }
    }
    pub fn broadcast(&self, msg: &str) {
        let clients = self.clients.lock().unwrap();
        for client in clients.iter() {
            let _ = client.do_send(WsMessage::Text(msg.to_owned()));
        }
    }

    pub fn broadcast_bytes(&self, data: &[u8]) {
        let clients = self.clients.lock().unwrap();
        for client in clients.iter() {
            let _ = client.do_send(WsMessage::Binary(data.to_vec()));
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
}

pub struct WsSession {
    pub broadcaster: Broadcaster,
}

impl Actor for WsSession {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = ctx.address().recipient();
        self.broadcaster.clients.lock().unwrap().push(addr);
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsSession {
    fn handle(
        &mut self,
        msg: Result<ws::Message, ws::ProtocolError>,
        ctx: &mut ws::WebsocketContext<Self>,
    ) {
        if let Ok(ws::Message::Text(text)) = msg {
            if text == "ping" {
                ctx.text("pong");
            }
        }
    }
}

impl Handler<WsMessage> for WsSession {
    type Result = ();

    fn handle(&mut self, msg: WsMessage, ctx: &mut ws::WebsocketContext<Self>) {
        match msg {
            WsMessage::Text(text) => ctx.text(text),
            WsMessage::Binary(data) => ctx.binary(data),
        }
    }
}

pub async fn ws_index(
    req: actix_web::HttpRequest,
    stream: actix_web::web::Payload,
    broadcaster: actix_web::web::Data<Broadcaster>,
) -> actix_web::Result<actix_web::HttpResponse> {
    let session = WsSession {
        broadcaster: broadcaster.get_ref().clone(),
    };
    ws::start(session, &req, stream)
}
