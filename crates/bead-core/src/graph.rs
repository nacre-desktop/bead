// SPDX-License-Identifier: MIT
//! Force-directed graph layout on one worker thread (spec 0232). Grid-binned repulsion
//! plus springs anneal until the moving nodes are quiet, then the thread parks on a
//! condvar at zero CPU until a topology, pin or restore command arrives. Positions reach
//! the UI as latest-only snapshots; a host wake callback fires once per pending snapshot.
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
};

pub type Point = [f32; 2];
type Wake = Arc<dyn Fn() + Send + Sync>;

/// Flat spatial bins shared by layout and UI hit testing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid(Vec<([i32; 2], u32)>);
impl Grid {
    pub fn cell(point: Point, cell_size: f32) -> [i32; 2] {
        [
            (point[0] / cell_size).floor() as i32,
            (point[1] / cell_size).floor() as i32,
        ]
    }
    pub fn update(&mut self, positions: &[Point], cell_size: f32) {
        self.0.clear();
        self.0.reserve(positions.len());
        self.0.extend(
            positions
                .iter()
                .enumerate()
                .map(|(i, &point)| (Self::cell(point, cell_size), i as u32)),
        );
        self.0.sort_unstable_by_key(|entry| entry.0);
    }
    pub fn at(&self, cell: [i32; 2]) -> impl Iterator<Item = usize> + '_ {
        let start = self.0.partition_point(|entry| entry.0 < cell);
        self.0[start..]
            .iter()
            .take_while(move |entry| entry.0 == cell)
            .map(|entry| entry.1 as usize)
    }
}

const CELL: f32 = 56.;
const REPULSION: f32 = 1200.;
const SPRING_LENGTH: f32 = 90.;
const SPRING_K: f32 = 0.003;
const DAMPING: f32 = 0.82;
const ALPHA_START: f32 = 0.18;
const ALPHA_DECAY: f32 = 0.998;
const ALPHA_FLOOR: f32 = 0.001;
/// Mean velocity² (world px²/step) under which the layout counts as quiet.
const QUIET_ENERGY: f32 = 1e-5;
const QUIET_STEPS: u32 = 12;
const MAX_STEPS: u32 = 3000;

/// Positions, velocities and topology, owned by the layout thread.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub positions: Vec<Point>,
    velocities: Vec<Point>,
    pub pinned: Vec<bool>,
    /// True while a node still settles; local reheats warm only a few nodes.
    heat: Vec<bool>,
    pub edges: Arc<[[u32; 2]]>,
    grid: Grid,
    force: Vec<Point>,
    degree: Vec<u32>,
    alpha: f32,
    quiet: u32,
    steps: u32,
}
/// A change the worker applies before its next step.
#[derive(Clone, Debug)]
pub enum Command {
    /// Replace the graph; `hot` marks nodes (and, implicitly, their neighbours) that settle again.
    /// Positions of every other node are kept exactly as given.
    Replace {
        positions: Vec<Point>,
        pinned: Vec<bool>,
        edges: Arc<[[u32; 2]]>,
        hot: Vec<bool>,
    },
    /// Move and/or pin one node (a drag); unpinning reheats it and its neighbours.
    Pin {
        node: u32,
        position: Option<Point>,
        pinned: bool,
    },
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub revision: u64,
    /// Sequence of the latest command applied before this snapshot was published.
    pub command_revision: u64,
    pub positions: Arc<[Point]>,
    pub settled: bool,
}
impl Layout {
    pub fn settled(&self) -> bool {
        self.quiet >= QUIET_STEPS || self.steps >= MAX_STEPS || self.heat.iter().all(|h| !h)
    }
    fn reheat(&mut self, hot: impl Fn(usize) -> bool) {
        for (i, heat) in self.heat.iter_mut().enumerate() {
            *heat |= hot(i);
        }
        for &[a, b] in self.edges.iter() {
            let (a, b) = (a as usize, b as usize);
            self.heat[a] |= hot(b);
            self.heat[b] |= hot(a);
        }
        self.alpha = ALPHA_START;
        self.quiet = 0;
        self.steps = 0;
    }
    pub fn apply(&mut self, command: Command) {
        match command {
            Command::Replace {
                positions,
                pinned,
                edges,
                hot,
            } => {
                let n = positions.len();
                let valid = pinned.len() == n
                    && hot.len() == n
                    && positions.iter().flatten().all(|v| v.is_finite())
                    && edges.iter().flatten().all(|i| (*i as usize) < n);
                if !valid {
                    return;
                }
                self.positions = positions;
                self.velocities = vec![[0., 0.]; n];
                self.pinned = pinned;
                self.heat = vec![false; n];
                self.edges = edges;
                self.grid.0.clear();
                self.force.resize(n, [0., 0.]);
                self.degree.resize(n, 0);
                self.degree.fill(0);
                for [a, b] in self.edges.iter() {
                    self.degree[*a as usize] += 1;
                    self.degree[*b as usize] += 1;
                }
                let warm = hot.iter().any(|hot| *hot);
                if !warm {
                    self.quiet = QUIET_STEPS;
                } else {
                    self.reheat(|i| hot[i]);
                }
            }
            Command::Pin {
                node,
                position,
                pinned,
            } => {
                let i = node as usize;
                if i >= self.positions.len() {
                    return;
                }
                if let Some(p) = position.filter(|p| p.iter().all(|v| v.is_finite())) {
                    self.positions[i] = p;
                }
                self.velocities[i] = [0., 0.];
                if self.pinned[i] && !pinned {
                    self.reheat(|node| node == i);
                }
                self.pinned[i] = pinned;
            }
        }
    }
    /// One integration step; returns the mean velocity² of the nodes still settling.
    pub fn step(&mut self) -> f32 {
        self.grid.update(&self.positions, CELL);
        self.force.fill([0., 0.]);
        for (i, p) in self.positions.iter().enumerate() {
            if !self.heat[i] || self.pinned[i] {
                continue;
            }
            let [cx, cy] = Grid::cell(*p, CELL);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let cell = [cx.saturating_add(dx), cy.saturating_add(dy)];
                    for j in self.grid.at(cell) {
                        if i == j {
                            continue;
                        }
                        let q = self.positions[j];
                        let (x, y) = (p[0] - q[0], p[1] - q[1]);
                        let d2 = x * x + y * y + 0.01;
                        if d2 < CELL * CELL * 2.25 {
                            let k = REPULSION / (d2 * d2.sqrt());
                            self.force[i][0] += x * k;
                            self.force[i][1] += y * k;
                        }
                    }
                }
            }
        }
        // Each edge contribution is divided by the endpoint degree so hubs do not
        // accumulate an unbounded net force.
        for [a, b] in self.edges.iter() {
            let (a, b) = (*a as usize, *b as usize);
            let (x, y) = (
                self.positions[b][0] - self.positions[a][0],
                self.positions[b][1] - self.positions[a][1],
            );
            let d = (x * x + y * y).sqrt().max(0.01);
            let k = ((d - SPRING_LENGTH) * SPRING_K).clamp(-0.6, 0.6);
            let ka = k / self.degree[a].max(1) as f32;
            let kb = k / self.degree[b].max(1) as f32;
            self.force[a][0] += x / d * ka;
            self.force[a][1] += y / d * ka;
            self.force[b][0] -= x / d * kb;
            self.force[b][1] -= y / d * kb;
        }
        let (mut kinetic, mut moving) = (0f64, 0usize);
        for (i, f) in self.force.iter().enumerate() {
            if self.pinned[i] || !self.heat[i] {
                continue;
            }
            let gain = self.alpha;
            let v = &mut self.velocities[i];
            v[0] = (v[0] + f[0].clamp(-30., 30.) * gain) * DAMPING;
            v[1] = (v[1] + f[1].clamp(-30., 30.) * gain) * DAMPING;
            self.positions[i][0] += v[0];
            self.positions[i][1] += v[1];
            kinetic += f64::from(v[0] * v[0] + v[1] * v[1]);
            moving += 1;
        }
        let energy = (kinetic / moving.max(1) as f64) as f32;
        self.alpha = (self.alpha * ALPHA_DECAY).max(ALPHA_FLOOR);
        self.steps += 1;
        self.quiet = if energy < QUIET_ENERGY {
            self.quiet + 1
        } else {
            0
        };
        if self.settled() {
            self.heat.fill(false);
            self.velocities.fill([0., 0.]);
        }
        energy
    }
}

#[derive(Default)]
struct State {
    commands: Vec<(u64, Command)>,
    next_command_revision: u64,
    stop: bool,
    snapshot: Option<Snapshot>,
    tid: Option<u64>,
}
#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}
impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}
/// Owns the layout thread; dropping it stops and joins the thread.
pub struct Worker {
    shared: Arc<Shared>,
    join: Option<thread::JoinHandle<()>>,
}
impl Worker {
    /// `wake` is called once whenever a snapshot becomes pending after the last one was taken.
    pub fn spawn(wake: Option<Wake>) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let thread_shared = shared.clone();
        let join = thread::Builder::new()
            .name("bead-graph-layout".into())
            .spawn(move || run(&thread_shared, wake))?;
        Ok(Self {
            shared,
            join: Some(join),
        })
    }
    /// Queue a change; reject snapshots older than the returned command revision.
    pub fn send(&self, command: Command) -> u64 {
        let mut inbox = self.shared.lock();
        inbox.next_command_revision += 1;
        let revision = inbox.next_command_revision;
        if matches!(&command, Command::Replace { .. }) {
            inbox.commands.clear();
        }
        inbox.commands.push((revision, command));
        self.shared.wake.notify_one();
        revision
    }
    /// The newest unread snapshot, consumed at most once per frame by the caller.
    pub fn take_snapshot(&self) -> Option<Snapshot> {
        self.shared.lock().snapshot.take()
    }
    pub fn thread_id(&self) -> Option<u64> {
        self.shared.lock().tid
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_one();
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}
fn run(shared: &Shared, wake: Option<Wake>) {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: gettid has no preconditions and only reads the calling thread's id.
        let tid = unsafe { libc::gettid() };
        shared.lock().tid = Some(tid as u64);
    }
    let mut layout = Layout::default();
    let mut revision = 0u64;
    let mut command_revision = 0u64;
    loop {
        let commands = {
            let mut inbox = shared.lock();
            while inbox.commands.is_empty() && !inbox.stop && layout.settled() {
                inbox = shared.wake.wait(inbox).unwrap_or_else(|e| e.into_inner());
            }
            if inbox.stop {
                return;
            }
            std::mem::take(&mut inbox.commands)
        };
        for (sequence, command) in commands {
            layout.apply(command);
            command_revision = sequence;
        }
        if !layout.settled() {
            layout.step();
        }
        revision += 1;
        let pending = {
            let mut state = shared.lock();
            let slot = &mut state.snapshot;
            let was_empty = slot.is_none();
            let reusable = slot.as_mut().and_then(|snapshot| {
                Arc::get_mut(&mut snapshot.positions)
                    .filter(|positions| positions.len() == layout.positions.len())
            });
            if let Some(positions) = reusable {
                positions.copy_from_slice(&layout.positions);
            } else {
                *slot = Some(Snapshot {
                    revision,
                    command_revision,
                    positions: layout.positions.as_slice().into(),
                    settled: layout.settled(),
                });
            }
            let snapshot = slot.as_mut().unwrap();
            snapshot.revision = revision;
            snapshot.command_revision = command_revision;
            snapshot.settled = layout.settled();
            was_empty
        };
        if pending && let Some(wake) = &wake {
            wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn seeded(n: usize, m: usize, seed: u64) -> (Vec<Point>, Vec<[u32; 2]>) {
        let mut s = seed;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            (s >> 40) as f32 / (1u32 << 24) as f32
        };
        let positions = (0..n)
            .map(|_| [next() * 600. - 300., next() * 400. - 200.])
            .collect();
        let edges = (0..m)
            .map(|_| {
                let a = (next() * n as f32) as u32 % n as u32;
                let mut b = (next() * n as f32) as u32 % n as u32;
                if a == b {
                    b = (b + 1) % n as u32;
                }
                [a, b]
            })
            .collect();
        (positions, edges)
    }
    fn replace(n: usize, m: usize, seed: u64) -> Command {
        let (positions, edges) = seeded(n, m, seed);
        Command::Replace {
            positions,
            pinned: vec![false; n],
            edges: edges.into(),
            hot: vec![true; n],
        }
    }
    fn settle(layout: &mut Layout) -> u32 {
        let mut steps = 0;
        while !layout.settled() {
            layout.step();
            steps += 1;
        }
        steps
    }
    fn thread_ticks(tid: u64) -> u64 {
        let stat = std::fs::read_to_string(format!("/proc/{}/task/{tid}/stat", std::process::id()))
            .unwrap();
        let fields: Vec<_> = stat
            .rsplit_once(')')
            .unwrap()
            .1
            .split_whitespace()
            .collect();
        fields[11].parse::<u64>().unwrap() + fields[12].parse::<u64>().unwrap()
    }
    fn wait_settled(worker: &Worker) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(snapshot) = worker.take_snapshot()
                && snapshot.settled
            {
                return snapshot;
            }
            assert!(Instant::now() < deadline, "layout did not settle in time");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn non_finite_replace_positions_leave_layout_unchanged() {
        let mut layout = Layout::default();
        layout.apply(Command::Replace {
            positions: vec![[1., 2.], [3., 4.]],
            pinned: vec![false, true],
            edges: Arc::from([[0, 1]]),
            hot: vec![true, false],
        });
        let before = layout.clone();

        layout.apply(Command::Replace {
            positions: vec![[f32::NAN, 0.], [3., 4.]],
            pinned: vec![false, false],
            edges: Arc::from([[0, 1]]),
            hot: vec![false, false],
        });

        assert_eq!(layout, before);
    }

    #[test]
    fn snapshots_identify_the_command_state_they_contain() {
        let worker = Worker::spawn(None).unwrap();
        let first = worker.send(Command::Replace {
            positions: vec![[1., 2.]],
            pinned: vec![true],
            edges: Arc::from([]),
            hot: vec![false],
        });
        let old = wait_settled(&worker);
        assert_eq!(old.command_revision, first);

        let fresh = worker.send(Command::Pin {
            node: 0,
            position: Some([5., 6.]),
            pinned: true,
        });
        assert!(fresh > old.command_revision);

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(snapshot) = worker.take_snapshot()
                && snapshot.command_revision == fresh
            {
                assert_eq!(&*snapshot.positions, &[[5., 6.]]);
                break;
            }
            assert!(Instant::now() < deadline, "fresh command was not reflected");
            thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn settles_under_the_step_cap_with_quiet_motion() {
        let mut layout = Layout::default();
        layout.apply(replace(80, 300, 4));
        let steps = settle(&mut layout);
        assert!(steps < MAX_STEPS, "ran to the cap after {steps} steps");
        assert!(layout.settled());
        assert!(layout.heat.iter().all(|h| !h));
    }
    #[test]
    fn pinned_nodes_hold_and_a_frozen_replace_keeps_positions() {
        let mut layout = Layout::default();
        layout.apply(replace(40, 120, 7));
        layout.apply(Command::Pin {
            node: 3,
            position: Some([5., 6.]),
            pinned: true,
        });
        settle(&mut layout);
        assert_eq!(layout.positions[3], [5., 6.]);
        let before = layout.positions.clone();
        // A rename or style change reaches the worker as nothing; a same-topology
        // replace with no hot nodes (pins or restore) keeps every position.
        layout.apply(Command::Replace {
            positions: before.clone(),
            pinned: layout.pinned.clone(),
            edges: layout.edges.clone(),
            hot: vec![false; 40],
        });
        assert!(layout.settled());
        assert_eq!(layout.positions, before);
    }
    #[test]
    fn adding_a_node_moves_only_its_neighbourhood() {
        let mut layout = Layout::default();
        layout.apply(replace(60, 150, 9));
        settle(&mut layout);
        let before = layout.positions.clone();
        let mut positions = before.clone();
        positions.push([before[0][0] + 20., before[0][1]]);
        let mut edges = layout.edges.to_vec();
        edges.push([0, 60]);
        let mut hot = vec![false; 61];
        hot[60] = true;
        layout.apply(Command::Replace {
            positions,
            pinned: vec![false; 61],
            edges: edges.into(),
            hot,
        });
        settle(&mut layout);
        let warm: std::collections::BTreeSet<usize> = layout
            .edges
            .iter()
            .filter_map(|[a, b]| {
                (*a == 60)
                    .then_some(*b as usize)
                    .or((*b == 60).then_some(*a as usize))
            })
            .chain([60])
            .collect();
        for (i, p) in before.iter().enumerate() {
            if !warm.contains(&i) {
                assert_eq!(layout.positions[i], *p, "node {i} moved on a local add");
            }
        }
        assert_ne!(layout.positions[60], [before[0][0] + 20., before[0][1]]);
    }
    #[test]
    fn worker_settles_publishes_and_parks_without_cpu_until_woken() {
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = wakes.clone();
        let wake: Wake = Arc::new(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        });
        let worker = Worker::spawn(Some(wake)).unwrap();
        worker.send(replace(80, 300, 4));
        let snapshot = wait_settled(&worker);
        assert_eq!(snapshot.positions.len(), 80);
        assert!(wakes.load(std::sync::atomic::Ordering::Relaxed) >= 1);
        let tid = worker.thread_id().unwrap();
        let before = thread_ticks(tid);
        thread::sleep(Duration::from_millis(200));
        assert_eq!(thread_ticks(tid), before, "parked worker used CPU");
        assert!(worker.take_snapshot().is_none());
        worker.send(Command::Pin {
            node: 1,
            position: Some([0., 0.]),
            pinned: true,
        });
        let woken = wait_settled(&worker);
        assert_eq!(woken.positions[1], [0., 0.]);
        assert!(woken.revision > snapshot.revision);
    }
    #[test]
    fn layout_round_trips_and_drop_joins() {
        let worker = Worker::spawn(None).unwrap();
        let positions = vec![[1., 2.], [3., 4.], [5., 6.]];
        worker.send(Command::Replace {
            positions: positions.clone(),
            pinned: vec![true, false, false],
            edges: Arc::from([[0, 1], [1, 2]]),
            hot: vec![false; 3],
        });
        let snapshot = wait_settled(&worker);
        assert_eq!(&*snapshot.positions, positions.as_slice());
        let shared = worker.shared.clone();
        drop(worker);
        assert_eq!(Arc::strong_count(&shared), 1, "layout thread did not exit");
    }
}
