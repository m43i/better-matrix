//! A loopback homeserver for exercising SDK requests without credentials or live services.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

pub(crate) struct Server {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Server {
    pub fn new(reply: impl Fn(&str) -> serde_json::Value + Send + 'static) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("listen on loopback");
        let address = listener.local_addr().expect("server address");
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let thread = std::thread::spawn(move || {
            'connections: for stream in listener.incoming() {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let mut stream = stream.expect("accepted connection");
                stream.set_read_timeout(Some(std::time::Duration::from_secs(2))).expect("read timeout");
                let mut request = Vec::new();
                let mut bytes = [0; 4096];
                loop {
                    // A client that gave up on its request (a cancelled long poll) sends nothing.
                    let Ok(n) = stream.read(&mut bytes) else { continue 'connections };
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&bytes[..n]);
                    let headers = String::from_utf8_lossy(&request);
                    if let Some(end) = headers.find("\r\n\r\n") {
                        let length = headers[..end]
                            .lines()
                            .find_map(|line| {
                                let (key, value) = line.split_once(':')?;
                                key.eq_ignore_ascii_case("content-length")
                                    .then(|| value.trim().parse::<usize>().expect("content length"))
                            })
                            .unwrap_or(0);
                        if request.len() >= end + 4 + length {
                            break;
                        }
                    }
                }
                let body = reply(&String::from_utf8_lossy(&request)).to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        Self { address, stopped, thread: Some(thread) }
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            thread.join().expect("test server stopped");
        }
    }
}

pub(crate) fn thread_response(request: &str) -> serde_json::Value {
    use serde_json::json;
    let root = json!({"type":"m.room.message","room_id":"!room:example.org","event_id":"$root","sender":"@b:example.org","origin_server_ts":1,"content":{"msgtype":"m.text","body":"Root"}});
    let reply = json!({"type":"m.room.message","room_id":"!room:example.org","event_id":"$reply","sender":"@b:example.org","origin_server_ts":2,"content":{"msgtype":"m.text","body":"Reply","m.relates_to":{"rel_type":"m.thread","event_id":"$root"}}});
    if request.contains("/sync?") {
        return json!({"next_batch":"sync","rooms":{"join":{"!room:example.org":{
            "state":{"events":[
                {"type":"m.room.create","state_key":"","sender":"@a:example.org","event_id":"$create","origin_server_ts":0,"content":{"creator":"@a:example.org","room_version":"10"}},
                {"type":"m.room.member","state_key":"@a:example.org","sender":"@a:example.org","event_id":"$join","origin_server_ts":0,"content":{"membership":"join","displayname":"Ada"}}
            ]},"timeline":{"events":[root],"limited":false,"prev_batch":"room-token"},"summary":{"m.joined_member_count":2}
        }}}});
    }
    if request.contains("/relations/") {
        return json!({"chunk":[reply]});
    }
    if request.contains("/event/") {
        return root;
    }
    if request.contains("/keys/upload") {
        return json!({"one_time_key_counts":{}});
    }
    if request.contains("/keys/query") {
        return json!({"device_keys":{}});
    }
    json!({})
}
