use kairos_domain::repositories::market_stream::{MarketEvent, MarketStream, StreamError};
use rand::RngCore;
use serde::Deserialize;
use std::time::{Duration, Instant};
use tungstenite::protocol::Message;
use url::Url;

const KUCOIN_BULLET_PUBLIC: &str = "https://api.kucoin.com/api/v1/bullet-public";

#[derive(Debug)]
pub struct KucoinPublicTickerStream {
    symbol: String,
    socket: tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
    ping_interval: Duration,
    last_ping: Instant,
}

impl KucoinPublicTickerStream {
    pub fn connect(symbol: String) -> Result<Self, String> {
        let (socket, ping_interval) = connect_socket(&symbol)?;
        Ok(Self {
            symbol,
            socket,
            ping_interval,
            last_ping: Instant::now(),
        })
    }
}

impl MarketStream for KucoinPublicTickerStream {
    fn next_event(&mut self) -> Result<MarketEvent, StreamError> {
        loop {
            if self.last_ping.elapsed() >= self.ping_interval {
                let id = format!("ping-{}", random_id());
                let payload = serde_json::json!({ "id": id, "type": "ping" }).to_string();
                self.socket
                    .send(Message::Text(payload))
                    .map_err(|e| StreamError::Disconnected(format!("ping failed: {e}")))?;
                self.last_ping = Instant::now();
            }

            let msg = self
                .socket
                .read()
                .map_err(|e| StreamError::Disconnected(e.to_string()))?;

            match msg {
                Message::Text(text) => {
                    if let Ok(envelope) = serde_json::from_str::<KucoinEnvelope>(&text) {
                        if envelope.r#type == "message"
                            && envelope.topic == format!("/market/ticker:{}", self.symbol)
                        {
                            let data = envelope.data.ok_or_else(|| {
                                StreamError::Protocol("ticker message missing data".to_string())
                            })?;
                            let ts = parse_kucoin_time_to_seconds(data.time)?;
                            let price = data
                                .price
                                .parse::<f64>()
                                .map_err(|e| StreamError::InvalidData(format!("bad price: {e}")))?;
                            return Ok(MarketEvent::Tick {
                                timestamp: ts,
                                price,
                            });
                        }
                        // Ignore other messages (welcome/ack/pong).
                        continue;
                    }
                    continue;
                }
                Message::Ping(payload) => {
                    // Reply to keep-alive.
                    let _ = self.socket.send(Message::Pong(payload));
                }
                Message::Pong(_) => {}
                Message::Binary(_) => {}
                Message::Close(_) => {
                    return Err(StreamError::Disconnected("server closed".to_string()));
                }
                Message::Frame(_) => {}
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct KucoinBulletResponse {
    code: String,
    data: KucoinBulletData,
}

#[derive(Debug, Deserialize)]
struct KucoinBulletData {
    token: String,
    #[serde(rename = "instanceServers")]
    instance_servers: Vec<KucoinInstanceServer>,
}

#[derive(Debug, Deserialize)]
struct KucoinInstanceServer {
    endpoint: String,
    #[serde(rename = "pingInterval")]
    ping_interval_ms: u64,
}

#[derive(Debug, Deserialize)]
struct KucoinEnvelope {
    #[serde(rename = "type")]
    r#type: String,
    topic: String,
    data: Option<KucoinTickerData>,
}

#[derive(Debug, Deserialize)]
struct KucoinTickerData {
    // KuCoin sends epoch in ms as an integer.
    time: i64,
    price: String,
}

fn connect_socket(
    symbol: &str,
) -> Result<
    (
        tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>,
        Duration,
    ),
    String,
> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| format!("failed to build reqwest client: {e}"))?;

    let resp: KucoinBulletResponse = client
        .post(KUCOIN_BULLET_PUBLIC)
        .send()
        .map_err(|e| format!("bullet-public request failed: {e}"))?
        .json()
        .map_err(|e| format!("bullet-public parse failed: {e}"))?;

    if resp.code != "200000" {
        return Err(format!("bullet-public error code: {}", resp.code));
    }
    let server = resp
        .data
        .instance_servers
        .first()
        .ok_or_else(|| "bullet-public missing instanceServers".to_string())?;

    let connect_id = random_id();
    let mut url =
        Url::parse(&server.endpoint).map_err(|e| format!("invalid ws endpoint URL: {e}"))?;
    url.query_pairs_mut()
        .append_pair("token", &resp.data.token)
        .append_pair("connectId", &connect_id);

    let (mut socket, _resp) =
        tungstenite::connect(url).map_err(|e| format!("ws connect failed: {e}"))?;

    let id = format!("sub-{}", random_id());
    let topic = format!("/market/ticker:{symbol}");
    let subscribe = serde_json::json!({
        "id": id,
        "type": "subscribe",
        "topic": topic,
        "privateChannel": false,
        "response": true
    })
    .to_string();
    socket
        .send(Message::Text(subscribe))
        .map_err(|e| format!("ws subscribe failed: {e}"))?;

    let ping_interval = Duration::from_millis(server.ping_interval_ms.max(1000));
    Ok((socket, ping_interval))
}

fn random_id() -> String {
    let mut rng = rand::thread_rng();
    let v = rng.next_u64();
    format!("{v:016x}")
}

fn parse_kucoin_time_to_seconds(ts: i64) -> Result<i64, StreamError> {
    if ts <= 0 {
        return Err(StreamError::InvalidData("timestamp <= 0".to_string()));
    }
    if ts >= 1_000_000_000_000i64 {
        Ok(ts / 1000)
    } else {
        Ok(ts)
    }
}
