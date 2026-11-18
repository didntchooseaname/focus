use std::io::Read;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::{Cancel, Config, Error};

const POLL: Duration = Duration::from_millis(20);
const READ_BUFFER: usize = 8 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
pub enum Method {
    Get,
    Post,
}

enum Message {
    Head {
        status: u16,
        retry_after: Option<u32>,
    },
    Body(Vec<u8>),
    End,
    Failed(String),
}

pub struct Response {
    pub status: u16,
    pub retry_after: Option<u32>,
    receiver: Receiver<Message>,
    finished: bool,
}

pub fn join(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

pub fn start(
    config: &Config,
    method: Method,
    url: &str,
    body: Option<String>,
    cancel: &Cancel,
) -> Result<Response, Error> {
    let (sender, receiver) = mpsc::channel();
    let worker_cancel = cancel.clone();
    let url = url.to_string();
    let headers = auth_headers(config);
    let timeout = Duration::from_secs(u64::from(config.timeout_secs.max(1)));
    std::thread::Builder::new()
        .name("focus-ai-http".into())
        .spawn(move || {
            work(
                method,
                &url,
                &headers,
                body,
                timeout,
                &worker_cancel,
                &sender,
            )
        })
        .map_err(|e| Error::Network(format!("network thread not created: {e}")))?;
    let mut response = Response {
        status: 0,
        retry_after: None,
        receiver,
        finished: false,
    };
    match response.wait(cancel, timeout)? {
        Some(Message::Head {
            status,
            retry_after,
        }) => {
            response.status = status;
            response.retry_after = retry_after;
            Ok(response)
        }
        Some(Message::Failed(text)) => Err(Error::Network(text)),
        _ => Err(Error::Network("response without headers".into())),
    }
}

fn auth_headers(config: &Config) -> Vec<(String, String)> {
    let key = config.api_key.expose();
    let mut headers = Vec::new();
    if config.provider == crate::Provider::Anthropic {
        headers.push(("x-api-key".to_string(), key.to_string()));
        headers.push((
            "anthropic-version".to_string(),
            crate::anthropic::VERSION.to_string(),
        ));
        if config.fallbacks {
            headers.push((
                "anthropic-beta".to_string(),
                crate::anthropic::FALLBACK_BETA.to_string(),
            ));
        }
    } else if !key.is_empty() {
        headers.push(("Authorization".to_string(), format!("Bearer {key}")));
    }
    headers
}

fn work(
    method: Method,
    url: &str,
    headers: &[(String, String)],
    body: Option<String>,
    timeout: Duration,
    cancel: &Cancel,
    sender: &mpsc::Sender<Message>,
) {
    let loopback = url.starts_with("http://127.0.0.1") || url.starts_with("http://localhost");
    let mut builder = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .user_agent("focus")
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_response(Some(timeout))
        .timeout_per_read(Some(timeout));
    if loopback {
        builder = builder.proxy(None);
    }
    let agent: ureq::Agent = builder.build().into();
    let result = match method {
        Method::Get => {
            let mut request = agent.get(url).header("Accept", "application/json");
            for (name, value) in headers {
                request = request.header(name.as_str(), value.as_str());
            }
            request.call()
        }
        Method::Post => {
            let mut request = agent
                .post(url)
                .header("Content-Type", "application/json")
                .header("Accept", "text/event-stream");
            for (name, value) in headers {
                request = request.header(name.as_str(), value.as_str());
            }
            request.send(body.unwrap_or_default())
        }
    };
    let response = match result {
        Ok(response) => response,
        Err(error) => {
            let _ = sender.send(Message::Failed(describe(&error)));
            return;
        }
    };
    let status = response.status().as_u16();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u32>().ok());
    if sender
        .send(Message::Head {
            status,
            retry_after,
        })
        .is_err()
    {
        return;
    }
    let mut reader = response.into_body().into_reader();
    let mut buffer = vec![0u8; READ_BUFFER];
    loop {
        if cancel.is_cancelled() {
            return;
        }
        match reader.read(&mut buffer) {
            Ok(0) => {
                let _ = sender.send(Message::End);
                return;
            }
            Ok(n) => {
                if sender.send(Message::Body(buffer[..n].to_vec())).is_err() {
                    return;
                }
            }
            Err(error) => {
                let _ = sender.send(Message::Failed(error.to_string()));
                return;
            }
        }
    }
}

fn describe(error: &ureq::Error) -> String {
    error.to_string()
}

impl Response {
    fn wait(&mut self, cancel: &Cancel, idle: Duration) -> Result<Option<Message>, Error> {
        let started = Instant::now();
        loop {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            match self.receiver.recv_timeout(POLL) {
                Ok(message) => return Ok(Some(message)),
                Err(RecvTimeoutError::Timeout) => {
                    if started.elapsed() >= idle {
                        return Err(Error::Network(format!(
                            "no data received for {} s",
                            idle.as_secs()
                        )));
                    }
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(Error::Network("network thread stopped".into()));
                }
            }
        }
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn next_chunk(
        &mut self,
        cancel: &Cancel,
        idle: Duration,
    ) -> Result<Option<Vec<u8>>, Error> {
        if self.finished {
            return Ok(None);
        }
        match self.wait(cancel, idle)? {
            Some(Message::Body(bytes)) => Ok(Some(bytes)),
            Some(Message::End) => {
                self.finished = true;
                Ok(None)
            }
            Some(Message::Failed(text)) => {
                self.finished = true;
                Err(Error::Network(text))
            }
            _ => Err(Error::Protocol("unexpected transport message".into())),
        }
    }

    pub fn read_all(&mut self, cancel: &Cancel, limit: usize) -> Result<String, Error> {
        let mut bytes = Vec::new();
        while let Some(chunk) = self.next_chunk(cancel, Duration::from_secs(60))? {
            if bytes.len() < limit {
                let room = limit - bytes.len();
                bytes.extend_from_slice(&chunk[..chunk.len().min(room)]);
            }
        }
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }
}

pub fn sleep_cancellable(duration: Duration, cancel: &Cancel) -> Result<(), Error> {
    let end = Instant::now() + duration;
    while Instant::now() < end {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        std::thread::sleep(POLL.min(end - Instant::now()));
    }
    if cancel.is_cancelled() {
        Err(Error::Cancelled)
    } else {
        Ok(())
    }
}
