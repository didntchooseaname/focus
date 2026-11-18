use focus_graph::{Graph, Params, Simulation, save_positions};
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

const GROWTH_STEP: Duration = Duration::from_micros(16_667);

enum Command {
    Pin(u32, Option<[f32; 2]>),
    Reheat(Params),
    Grow(Vec<u32>),
    EndGrowth,
    Wake,
}

pub(crate) type Step = (Arc<Vec<[f32; 2]>>, Option<usize>);

struct Shared {
    positions: Mutex<Step>,
    version: AtomicU64,
    settled: AtomicBool,
    grow_to: AtomicUsize,
}

pub(crate) struct Worker {
    commands: Sender<Command>,
    shared: Arc<Shared>,
}

impl Worker {
    pub fn spawn(graph: Arc<Graph>, sim: Simulation, cache_dir: Option<PathBuf>) -> Worker {
        let shared = Arc::new(Shared {
            positions: Mutex::new((Arc::new(sim.positions().to_vec()), sim.born())),
            version: AtomicU64::new(1),
            settled: AtomicBool::new(sim.is_settled()),
            grow_to: AtomicUsize::new(0),
        });
        let (commands, rx) = channel();
        let thread_shared = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("focus-graph-layout".into())
            .spawn(move || run(sim, graph, rx, thread_shared, cache_dir));
        if let Err(error) = spawned {
            eprintln!("focus-view-graph: layout thread: {error}");
            shared.settled.store(true, Ordering::Release);
        }
        Worker { commands, shared }
    }

    pub fn version(&self) -> u64 {
        self.shared.version.load(Ordering::Acquire)
    }

    pub fn positions(&self) -> Arc<Vec<[f32; 2]>> {
        self.shared.positions.lock().0.clone()
    }

    pub fn snapshot(&self) -> Step {
        self.shared.positions.lock().clone()
    }

    pub fn is_settled(&self) -> bool {
        self.shared.settled.load(Ordering::Acquire)
    }

    pub fn pin(&self, node: u32, position: Option<[f32; 2]>) {
        self.shared.settled.store(false, Ordering::Release);
        let _ = self.commands.send(Command::Pin(node, position));
    }

    pub fn reheat(&self, params: Params) {
        self.shared.settled.store(false, Ordering::Release);
        let _ = self.commands.send(Command::Reheat(params));
    }

    pub fn grow(&self, order: Vec<u32>) {
        self.shared.grow_to.store(0, Ordering::SeqCst);
        self.shared.settled.store(false, Ordering::Release);
        let _ = self.commands.send(Command::Grow(order));
    }

    pub fn grow_to(&self, count: usize) {
        if self.shared.grow_to.swap(count, Ordering::SeqCst) != count
            && self.shared.settled.load(Ordering::SeqCst)
        {
            let _ = self.commands.send(Command::Wake);
        }
    }

    pub fn end_growth(&self) {
        self.shared.settled.store(false, Ordering::Release);
        let _ = self.commands.send(Command::EndGrowth);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Pace {
    Free,
    Growing,
    Finishing,
}

fn apply(sim: &mut Simulation, command: Command, pace: &mut Pace) {
    match command {
        Command::Pin(node, position) => sim.pin(node, position),
        Command::Reheat(params) => sim.reheat(params),
        Command::Grow(order) => {
            *pace = Pace::Growing;
            sim.grow(order);
        }
        Command::EndGrowth => {
            let complete = sim.born() == Some(sim.positions().len());
            *pace = if complete {
                Pace::Finishing
            } else {
                Pace::Free
            };
            sim.end_growth();
        }
        Command::Wake => {}
    }
}

fn run(
    mut sim: Simulation,
    graph: Arc<Graph>,
    rx: Receiver<Command>,
    shared: Arc<Shared>,
    cache_dir: Option<PathBuf>,
) {
    let mut saved = sim.is_settled();
    let mut pace = Pace::Free;
    let mut next_step = Instant::now();
    loop {
        if sim.is_settled() {
            if pace == Pace::Finishing {
                pace = Pace::Free;
            }
            if !saved && pace != Pace::Growing {
                saved = true;
                if let Some(dir) = &cache_dir
                    && let Err(error) = save_positions(dir, &graph, sim.positions())
                {
                    eprintln!("focus-view-graph: {error:#}");
                }
            }
            shared.settled.store(true, Ordering::SeqCst);
            sim.grow_to(shared.grow_to.load(Ordering::SeqCst));
            if sim.is_settled() {
                match rx.recv() {
                    Ok(command) => apply(&mut sim, command, &mut pace),
                    Err(_) => return,
                }
            }
        }
        loop {
            match rx.try_recv() {
                Ok(command) => apply(&mut sim, command, &mut pace),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        sim.grow_to(shared.grow_to.load(Ordering::SeqCst));
        if sim.is_settled() {
            continue;
        }
        if pace != Pace::Free {
            let now = Instant::now();
            if let Some(wait) = next_step
                .checked_duration_since(now)
                .filter(|w| !w.is_zero())
            {
                match rx.recv_timeout(wait) {
                    Ok(command) => {
                        apply(&mut sim, command, &mut pace);
                        continue;
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            next_step = (next_step + GROWTH_STEP).max(now);
        }
        saved = false;
        sim.step(1);
        *shared.positions.lock() = (Arc::new(sim.positions().to_vec()), sim.born());
        shared.version.fetch_add(1, Ordering::AcqRel);
        if sim.is_settled() {
            shared.settled.store(true, Ordering::Release);
        }
    }
}
