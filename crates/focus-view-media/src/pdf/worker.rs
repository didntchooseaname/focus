use std::collections::VecDeque;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use super::nav::{self, Link, OutlineItem, Pages};
use super::text::PageText;
use super::{Bitmap, MAX_FILE_BYTES, PdfError, Region, Renderer};

pub const WORKER_ARG: &str = "--pdf-worker";
#[cfg(target_os = "linux")]
const ADDRESS_SPACE: u64 = 4 << 30;
const MAX_FRAME: u32 = 256 << 20;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Request {
    Open { path: PathBuf },
    Render { page: usize, region: Region },
    Text { page: usize },
    Links { page: usize },
    Outline,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Reply {
    Opened { sizes: Vec<(f32, f32)> },
    Failed(PdfError),
    Bitmap { width: u32, height: u32 },
    Text(PageText),
    Links(Vec<Link>),
    Outline(Vec<OutlineItem>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Opened { sizes: Vec<(f32, f32)> },
    Bitmap(Bitmap),
    Text(PageText),
    Links(Vec<Link>),
    Outline(Vec<OutlineItem>),
}

fn write_frame(out: &mut impl Write, reply: &impl Serialize, payload: &[u8]) -> io::Result<()> {
    let json = serde_json::to_vec(reply).map_err(io::Error::other)?;
    out.write_all(&(json.len() as u32).to_le_bytes())?;
    out.write_all(&json)?;
    out.write_all(&(payload.len() as u32).to_le_bytes())?;
    out.write_all(payload)?;
    out.flush()
}

fn read_frame<T: for<'de> Deserialize<'de>>(
    input: &mut impl Read,
) -> io::Result<Option<(T, Vec<u8>)>> {
    let mut length = [0u8; 4];
    match input.read_exact(&mut length) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let read_part = |input: &mut dyn Read, length: [u8; 4]| -> io::Result<Vec<u8>> {
        let length = u32::from_le_bytes(length);
        if length > MAX_FRAME.max(super::MAX_BITMAP_SIDE * super::MAX_BITMAP_SIDE * 4) {
            return Err(io::Error::other(format!("frame of {length} bytes")));
        }
        let mut buffer = vec![0; length as usize];
        input.read_exact(&mut buffer)?;
        Ok(buffer)
    };
    let json = read_part(input, length)?;
    input.read_exact(&mut length)?;
    let payload = read_part(input, length)?;
    let message = serde_json::from_slice(&json).map_err(io::Error::other)?;
    Ok(Some((message, payload)))
}

fn read_file(path: &Path) -> Result<Vec<u8>, PdfError> {
    let size = std::fs::metadata(path)
        .map_err(|_| PdfError::Unreadable)?
        .len();
    if size > MAX_FILE_BYTES {
        return Err(PdfError::TooLarge);
    }
    std::fs::read(path).map_err(|_| PdfError::Unreadable)
}

pub fn serve(mut input: impl Read, mut output: impl Write) -> io::Result<()> {
    let Some((Request::Open { path }, _)) = read_frame::<Request>(&mut input)? else {
        return Ok(());
    };
    let pdf = match read_file(&path).and_then(super::open) {
        Ok(pdf) => pdf,
        Err(error) => return write_frame(&mut output, &Reply::Failed(error), &[]),
    };
    let pages = Pages::of(&pdf);
    write_frame(
        &mut output,
        &Reply::Opened {
            sizes: super::page_sizes(&pdf),
        },
        &[],
    )?;
    let renderer = Renderer::new();
    let all = pdf.pages();
    while let Some((request, _)) = read_frame::<Request>(&mut input)? {
        let page = |index: usize| all.get(index).ok_or(PdfError::Unreadable);
        match request {
            Request::Open { .. } => {
                write_frame(&mut output, &Reply::Failed(PdfError::Unreadable), &[])?
            }
            Request::Render {
                page: index,
                region,
            } => match page(index) {
                Ok(page) => {
                    let bitmap = renderer.render(page, region);
                    write_frame(
                        &mut output,
                        &Reply::Bitmap {
                            width: bitmap.width,
                            height: bitmap.height,
                        },
                        &bitmap.bgra,
                    )?
                }
                Err(e) => write_frame(&mut output, &Reply::Failed(e), &[])?,
            },
            Request::Text { page: index } => match page(index) {
                Ok(page) => write_frame(&mut output, &Reply::Text(renderer.text(page)), &[])?,
                Err(e) => write_frame(&mut output, &Reply::Failed(e), &[])?,
            },
            Request::Links { page: index } => match page(index) {
                Ok(page) => write_frame(
                    &mut output,
                    &Reply::Links(nav::links(&pdf, page, &pages)),
                    &[],
                )?,
                Err(e) => write_frame(&mut output, &Reply::Failed(e), &[])?,
            },
            Request::Outline => write_frame(
                &mut output,
                &Reply::Outline(nav::outline(&pdf, &pages)),
                &[],
            )?,
        }
    }
    Ok(())
}

pub fn serve_stdio() -> i32 {
    limit_memory();
    let input = BufReader::new(io::stdin().lock());
    let output = BufWriter::new(io::stdout().lock());
    match serve(input, output) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("focus: PDF worker: {error}");
            1
        }
    }
}

#[cfg(target_os = "linux")]
fn limit_memory() {
    let limit = libc::rlimit {
        rlim_cur: ADDRESS_SPACE,
        rlim_max: ADDRESS_SPACE,
    };
    // SAFETY: setrlimit reads the struct it is given and keeps no reference to it.
    if unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) } != 0 {
        eprintln!(
            "focus: PDF worker: no memory limit: {}",
            io::Error::last_os_error()
        );
    }
}

#[cfg(not(target_os = "linux"))]
fn limit_memory() {}

type Waiting = smol::channel::Sender<Result<Answer, PdfError>>;

struct Queue {
    waiting: VecDeque<Waiting>,
    stopped: bool,
}

pub struct Client {
    input: Mutex<Box<dyn Write + Send>>,
    queue: Arc<Mutex<Queue>>,
    child: Option<Child>,
}

impl Client {
    pub fn spawn(
        path: &Path,
    ) -> io::Result<(Client, smol::channel::Receiver<Result<Answer, PdfError>>)> {
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg(WORKER_ARG)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn()?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(io::Error::other("worker without pipes"));
        };
        let client = Client::over(output, input, Some(child));
        let opened = client.send(Request::Open {
            path: path.to_path_buf(),
        });
        Ok((client, opened))
    }

    pub fn over(
        output: impl Read + Send + 'static,
        input: impl Write + Send + 'static,
        child: Option<Child>,
    ) -> Client {
        let queue = Arc::new(Mutex::new(Queue {
            waiting: VecDeque::new(),
            stopped: false,
        }));
        let reader_queue = queue.clone();
        std::thread::Builder::new()
            .name("focus-pdf-replies".into())
            .spawn(move || read_replies(BufReader::new(output), reader_queue))
            .map_err(|e| eprintln!("focus: PDF replies thread: {e}"))
            .ok();
        Client {
            input: Mutex::new(Box::new(BufWriter::new(input))),
            queue,
            child,
        }
    }

    pub fn send(&self, request: Request) -> smol::channel::Receiver<Result<Answer, PdfError>> {
        let (tx, rx) = smol::channel::bounded(1);
        let Ok(mut input) = self.input.lock() else {
            let _ = tx.try_send(Err(PdfError::WorkerStopped));
            return rx;
        };
        match self.queue.lock() {
            Ok(mut queue) if !queue.stopped => queue.waiting.push_back(tx),
            _ => {
                let _ = tx.try_send(Err(PdfError::WorkerStopped));
                return rx;
            }
        }
        if write_frame(&mut *input, &request, &[]).is_err()
            && let Ok(mut queue) = self.queue.lock()
        {
            queue.stopped = true;
            for waiting in queue.waiting.drain(..) {
                let _ = waiting.try_send(Err(PdfError::WorkerStopped));
            }
        }
        rx
    }

    pub fn waiting(&self) -> usize {
        self.queue.lock().map_or(0, |q| q.waiting.len())
    }
}

fn read_replies(mut output: impl Read, queue: Arc<Mutex<Queue>>) {
    loop {
        let frame = read_frame::<Reply>(&mut output);
        let Ok(mut queue) = queue.lock() else { return };
        let answer = match frame {
            Ok(Some((reply, payload))) => match reply {
                Reply::Opened { sizes } => Ok(Answer::Opened { sizes }),
                Reply::Failed(error) => Err(error),
                Reply::Bitmap { width, height }
                    if payload.len() == (width as usize) * (height as usize) * 4 =>
                {
                    Ok(Answer::Bitmap(Bitmap {
                        width,
                        height,
                        bgra: payload,
                    }))
                }
                Reply::Bitmap { .. } => Err(PdfError::Unreadable),
                Reply::Text(text) => Ok(Answer::Text(text)),
                Reply::Links(links) => Ok(Answer::Links(links)),
                Reply::Outline(items) => Ok(Answer::Outline(items)),
            },
            Ok(None) | Err(_) => {
                queue.stopped = true;
                for waiting in queue.waiting.drain(..) {
                    let _ = waiting.try_send(Err(PdfError::WorkerStopped));
                }
                return;
            }
        };
        if let Some(waiting) = queue.waiting.pop_front() {
            let _ = waiting.try_send(answer);
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::sample;

    fn sample_file(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/test-media")
            .join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.pdf");
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn in_thread() -> Client {
        let (request_read, request_write) = io::pipe().unwrap();
        let (reply_read, reply_write) = io::pipe().unwrap();
        std::thread::spawn(move || serve(BufReader::new(request_read), reply_write));
        Client::over(reply_read, request_write, None)
    }

    fn answer(rx: smol::channel::Receiver<Result<Answer, PdfError>>) -> Result<Answer, PdfError> {
        smol::block_on(rx.recv()).unwrap()
    }

    #[test]
    fn a_worker_answers_in_order() {
        let client = in_thread();
        let path = sample_file("order", &sample::document());
        let opened = client.send(Request::Open { path });
        let outline = client.send(Request::Outline);
        let bitmap = client.send(Request::Render {
            page: 2,
            region: Region::whole((842., 595.), 0.25).unwrap(),
        });
        let text = client.send(Request::Text { page: 2 });
        let links = client.send(Request::Links { page: 0 });
        let missing = client.send(Request::Text { page: 9 });
        assert_eq!(
            answer(opened),
            Ok(Answer::Opened {
                sizes: vec![(595., 842.), (595., 842.), (842., 595.)]
            })
        );
        assert!(matches!(answer(outline), Ok(Answer::Outline(items)) if items.len() == 3));
        assert!(
            matches!(answer(bitmap), Ok(Answer::Bitmap(b)) if (b.width, b.height) == (211, 149))
        );
        assert!(matches!(answer(text), Ok(Answer::Text(t)) if t.slice(0, 5) == "Annex"));
        assert!(matches!(answer(links), Ok(Answer::Links(l)) if l.len() == 2));
        assert_eq!(answer(missing), Err(PdfError::Unreadable));
    }

    #[test]
    fn a_protected_document_is_refused_in_the_worker() {
        let client = in_thread();
        let path = sample_file("protected", &sample::protected());
        assert_eq!(
            answer(client.send(Request::Open { path })),
            Err(PdfError::Protected)
        );
        assert_eq!(
            answer(client.send(Request::Outline)),
            Err(PdfError::WorkerStopped)
        );
    }

    #[test]
    fn a_worker_that_stops_fails_every_waiting_request() {
        let (request_read, request_write) = io::pipe().unwrap();
        let (reply_read, reply_write) = io::pipe().unwrap();
        std::thread::spawn(move || {
            let mut input = BufReader::new(request_read);
            let _ = read_frame::<Request>(&mut input);
            drop(reply_write);
        });
        let client = Client::over(reply_read, request_write, None);
        let first = client.send(Request::Outline);
        assert_eq!(answer(first), Err(PdfError::WorkerStopped));
        assert_eq!(
            answer(client.send(Request::Text { page: 0 })),
            Err(PdfError::WorkerStopped)
        );
        assert_eq!(client.waiting(), 0);
    }
}
