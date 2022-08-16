use std::time::{Duration, Instant};

use actix::prelude::*;
use actix_web::{web, Error, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use futures::stream::Stream;

pub struct EventStreamSocket {
    /// Client must send ping at least once per 30 seconds (CLIENT_TIMEOUT),
    /// otherwise we drop connection.
    hb: Instant,
    hb_interval: Duration,
    client_timeout: Duration,
}

impl EventStreamSocket {
    pub fn new(hb_interval: Duration, client_timeout: Duration) -> Self {
        Self {
            hb: Instant::now(),
            hb_interval,
            client_timeout,
        }
    }

    pub fn start(
        self,
        req: &HttpRequest,
        stream: web::Payload,
        event_stream: impl Stream<Item = Result<Vec<u8>, Box<dyn std::error::Error>>> + 'static,
        headers: Vec<(&'static str, String)>,
    ) -> Result<HttpResponse, Error> {
        let mut res = ws::handshake(req)?;
        headers
            .into_iter()
            .fold(&mut res, |r, v| r.append_header(v));

        Ok(
            res.streaming(ws::WebsocketContext::with_factory(stream, |ctx| {
                ctx.add_stream(event_stream);
                self
            })),
        )
    }

    /// helper method that sends ping to client every second.
    ///
    /// also this method checks heartbeats from client
    fn hb(&self, ctx: &mut <Self as Actor>::Context) {
        let client_timeout = self.client_timeout;
        ctx.run_interval(self.hb_interval, move |act, ctx| {
            // check client heartbeats
            if Instant::now().duration_since(act.hb) > client_timeout {
                // heartbeat timed out
                log::trace!("Websocket Client heartbeat failed, disconnecting!");

                // stop actor
                ctx.stop();

                // don't try to send a ping
                return;
            }

            ctx.ping(b"");
        });
    }
}

impl Actor for EventStreamSocket {
    type Context = ws::WebsocketContext<Self>;

    /// Method is called on actor start. We start the heartbeat process here.
    fn started(&mut self, ctx: &mut Self::Context) {
        log::trace!("EventStreamSocket started");
        self.hb(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::trace!("EventStreamSocket finished");
    }
}

impl StreamHandler<Result<Vec<u8>, Box<dyn std::error::Error>>> for EventStreamSocket {
    fn handle(
        &mut self,
        msg: Result<Vec<u8>, Box<dyn std::error::Error>>,
        ctx: &mut Self::Context,
    ) {
        match msg {
            Ok(event) => ctx.binary(event),
            _ => ctx.stop(),
        }
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for EventStreamSocket {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        // process websocket messages
        match msg {
            Ok(ws::Message::Pong(_)) => {
                self.hb = Instant::now();
            }
            _ => ctx.stop(),
        }
    }
}
