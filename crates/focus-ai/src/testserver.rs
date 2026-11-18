use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub enum Step {
    Send(Vec<u8>),
    Sleep(u64),
}

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub delay_before_headers_ms: u64,
    pub steps: Vec<Step>,
}

impl Reply {
    pub fn status(status: u16, body: &str) -> Reply {
        Reply {
            status,
            headers: vec![
                ("Content-Type".into(), "application/json".into()),
                ("Content-Length".into(), body.len().to_string()),
            ],
            delay_before_headers_ms: 0,
            steps: vec![Step::Send(body.as_bytes().to_vec())],
        }
    }

    pub fn sse(events: &[&str]) -> Reply {
        let mut steps: Vec<Step> = events
            .iter()
            .map(|e| Step::Send(format!("data: {e}\n\n").into_bytes()))
            .collect();
        steps.push(Step::Send(b"data: [DONE]\n\n".to_vec()));
        Reply {
            status: 200,
            headers: vec![("Content-Type".into(), "text/event-stream".into())],
            delay_before_headers_ms: 0,
            steps,
        }
    }

    pub fn raw(steps: Vec<Step>) -> Reply {
        Reply {
            status: 200,
            headers: vec![("Content-Type".into(), "text/event-stream".into())],
            delay_before_headers_ms: 0,
            steps,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Reply {
        self.headers.push((name.into(), value.into()));
        self
    }
}

pub struct Server {
    pub url: String,
    requests: Arc<Mutex<Vec<(String, String)>>>,
}

impl Server {
    pub fn start(replies: Vec<Reply>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").expect("local port");
        let url = format!("http://{}", listener.local_addr().expect("local address"));
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        std::thread::spawn(move || {
            let mut replies = replies.into_iter();
            for stream in listener.incoming() {
                let Ok(stream) = stream else { return };
                let Some(reply) = replies.next() else { return };
                let log = log.clone();
                std::thread::spawn(move || serve(stream, reply, &log));
            }
        });
        Server { url, requests }
    }

    pub fn requests(&self) -> Vec<(String, String)> {
        self.requests.lock().unwrap().clone()
    }
}

fn serve(mut stream: TcpStream, reply: Reply, log: &Mutex<Vec<(String, String)>>) {
    let Some((head, body)) = read_request(&mut stream) else {
        return;
    };
    log.lock().unwrap().push((head, body));
    std::thread::sleep(Duration::from_millis(reply.delay_before_headers_ms));
    let mut header = format!("HTTP/1.1 {} Test\r\nConnection: close\r\n", reply.status);
    for (name, value) in &reply.headers {
        header.push_str(&format!("{name}: {value}\r\n"));
    }
    header.push_str("\r\n");
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }
    for step in reply.steps {
        match step {
            Step::Send(bytes) => {
                if stream
                    .write_all(&bytes)
                    .and_then(|()| stream.flush())
                    .is_err()
                {
                    return;
                }
            }
            Step::Sleep(ms) => std::thread::sleep(Duration::from_millis(ms)),
        }
    }
}

fn read_request(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    let end = loop {
        let n = stream.read(&mut buffer).ok()?;
        if n == 0 {
            return None;
        }
        data.extend_from_slice(&buffer[..n]);
        if let Some(position) = data.windows(4).position(|w| w == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let head = String::from_utf8_lossy(&data[..end]).into_owned();
    let length = head
        .lines()
        .find_map(|l| {
            l.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|v| v.trim().parse::<usize>().unwrap_or(0))
        })
        .unwrap_or(0);
    while data.len() < end + length {
        let n = stream.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        data.extend_from_slice(&buffer[..n]);
    }
    Some((head, String::from_utf8_lossy(&data[end..]).into_owned()))
}
