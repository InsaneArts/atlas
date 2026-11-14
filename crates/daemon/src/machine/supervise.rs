//! What a machine guest's commands and jobs run under: a supervisor that
//! leads their process group and ends it the moment the daemon that
//! started it is gone or stops renewing its lease. The daemon's own
//! watchdog cannot stop work once the daemon is gone, and a new daemon may
//! never come; the supervisor never waits for one.
//!
//! Its standard input is the lifeline, its own alone: the daemon writes
//! [`RENEW`] while the generation the work belongs to is live. The pipe
//! closing, a lease gone unrenewed, or anything else on it, ends every
//! process the supervisor started: its group, and every descendant it has
//! seen leave the group. It goes on supervising until all of them are gone,
//! the program itself ending first or not.
//!
//! macOS gives a process no containment that survives both leaving its
//! group and losing its parent: a descendant that does both between two
//! looks is not seen.
use atlas_core::{
    error::Result,
    process::{Info, Instance},
};
use atlas_session::clock::{Clock, Continuous};
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::{Pid, getpgrp},
};
use std::{
    collections::HashSet,
    io::{ErrorKind, Read},
    process::{Command, ExitStatus, Stdio},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

/// What the daemon writes to renew the lease.
pub const RENEW: u8 = b'.';

/// How often the supervisor looks, at most.
const LOOK: Duration = Duration::from_millis(100);

/// Run `program` with `arguments` in this process's own group, under
/// `lifeline`, ending everything it started when no renewal comes within
/// `lease` on the continuous clock. Returns once the program and all it
/// left running have ended.
///
/// # Errors
///
/// Never returns an error: anything that goes wrong ends the work.
pub fn supervise(
    program: &str,
    arguments: &[String],
    lifeline: impl Read + Send + 'static,
    lease: Duration,
) -> Result<ExitStatus> {
    let work = Arc::new(Work::new(Live, getpgrp()));
    // The lease runs from before anything starts, and a clock that cannot
    // be read starts nothing.
    let lease = Arc::new(Lease::start(Continuous, lease).unwrap_or_else(|| work.end()));
    if !listen(lifeline, Arc::clone(&lease)) || !watch(Arc::clone(&lease), Arc::clone(&work)) {
        work.end();
    }
    let mut command = Command::new(program);
    command.args(arguments).stdin(Stdio::null());
    // Checked again once nothing else may start: a lease that ran out while
    // waiting starts nothing.
    let Ok(Some(mut child)) = atlas_core::spawn::spawn_if(&mut command, || lease.live()) else {
        work.end();
    };
    let mut status = None;
    loop {
        if status.is_none() {
            status = child.try_wait().unwrap_or_else(|_| work.end());
        }
        let left = work.look().unwrap_or_else(|| work.end());
        if let Some(status) = status
            && !left
        {
            return Ok(status);
        }
        std::thread::sleep(LOOK);
    }
}

/// The lease, as the lifeline has kept it: when it runs out, on its clock,
/// or none once it is lost for good.
struct Lease<C> {
    clock: C,
    length: Duration,
    due: Mutex<Option<Duration>>,
    changed: Condvar,
}

impl<C: Clock> Lease<C> {
    /// A lease of `length` from now; none when `clock` cannot be read.
    fn start(clock: C, length: Duration) -> Option<Self> {
        let due = clock.now()?.checked_add(length)?;
        Some(Self {
            clock,
            length,
            due: Mutex::new(Some(due)),
            changed: Condvar::new(),
        })
    }

    fn due(&self) -> MutexGuard<'_, Option<Duration>> {
        self.due.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A renewal received now runs the lease on from now; one received
    /// after it ran out, or with the clock unreadable, loses it.
    fn renew(&self) {
        let now = self.clock.now();
        let mut due = self.due();
        *due = match (*due, now) {
            (Some(until), Some(now)) if now < until => now.checked_add(self.length),
            _ => None,
        };
        self.changed.notify_all();
    }

    /// The daemon is gone: nothing renews it again.
    fn lose(&self) {
        *self.due() = None;
        self.changed.notify_all();
    }

    /// Whether the lease still holds now.
    fn live(&self) -> bool {
        let due = *self.due();
        due.zip(self.clock.now())
            .is_some_and(|(until, now)| now < until)
    }

    /// Wait until the lease is lost or runs out.
    fn ended(&self) {
        let mut due = self.due();
        loop {
            let Some(until) = *due else {
                return;
            };
            let Some(left) = self
                .clock
                .now()
                .and_then(|now| until.checked_sub(now))
                .filter(|left| !left.is_zero())
            else {
                return;
            };
            // The wait's own clock may not count sleep: it is never trusted
            // for longer than a look.
            due = self
                .changed
                .wait_timeout(due, left.min(LOOK))
                .map_or_else(|poisoned| poisoned.into_inner().0, |(due, _)| due);
        }
    }
}

/// Read the lifeline on a thread of its own, each renewal kept as it is
/// received. False when the thread cannot be started.
fn listen<C: Clock + 'static>(
    mut lifeline: impl Read + Send + 'static,
    lease: Arc<Lease<C>>,
) -> bool {
    std::thread::Builder::new()
        .spawn(move || {
            let mut byte = [0; 1];
            loop {
                match lifeline.read(&mut byte) {
                    Ok(1) if byte[0] == RENEW => lease.renew(),
                    Err(error) if error.kind() == ErrorKind::Interrupted => {}
                    Ok(_) | Err(_) => return lease.lose(),
                }
            }
        })
        .is_ok()
}

/// End the work on a thread of its own the moment the lease ends, however
/// the supervisor's own thread is held up. False when the thread cannot be
/// started.
fn watch<C: Clock + 'static, K: Kernel + 'static>(
    lease: Arc<Lease<C>>,
    work: Arc<Work<K>>,
) -> bool {
    std::thread::Builder::new()
        .spawn(move || {
            lease.ended();
            work.end();
        })
        .is_ok()
}

/// What the kernel says of processes, as the supervisor asks it.
trait Kernel: Send + Sync {
    /// Every process in group `pgid`.
    fn group(&self, pgid: i32) -> atlas_core::error::Result<Vec<Info>>;
    /// The IDs of the processes whose parent is `pid`.
    fn children(&self, pid: i32) -> atlas_core::error::Result<Vec<i32>>;
    /// Process `pid`, none when there is none.
    fn info(&self, pid: i32) -> atlas_core::error::Result<Option<Info>>;
    /// Whether this very process is known to have ended.
    fn ended(&self, instance: &Instance) -> bool;
}

/// The kernel itself.
struct Live;

impl Kernel for Live {
    fn group(&self, pgid: i32) -> atlas_core::error::Result<Vec<Info>> {
        atlas_core::process::group(pgid)
    }

    fn children(&self, pid: i32) -> atlas_core::error::Result<Vec<i32>> {
        atlas_core::process::children(pid)
    }

    fn info(&self, pid: i32) -> atlas_core::error::Result<Option<Info>> {
        atlas_core::process::info(pid)
    }

    fn ended(&self, instance: &Instance) -> bool {
        instance.ended()
    }
}

/// What the supervisor started: its group, and every process it has seen
/// descend from it, kept by identity so one that left the group is still
/// known. The supervisor itself is never among them.
struct Work<K> {
    kernel: K,
    group: Pid,
    own: i32,
    seen: Mutex<HashSet<Instance>>,
}

impl<K: Kernel> Work<K> {
    fn new(kernel: K, group: Pid) -> Self {
        Self {
            kernel,
            group,
            own: i32::try_from(std::process::id()).unwrap_or(i32::MAX),
            seen: Mutex::new(HashSet::new()),
        }
    }

    fn seen(&self) -> MutexGuard<'_, HashSet<Instance>> {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Look again: whether anything it started still runs. None when the
    /// kernel would not say, which is never taken for nothing left. What it
    /// finds is kept as it is found; asking the kernel holds nothing up.
    fn look(&self) -> Option<bool> {
        let before = self.seen().clone();
        let mut looking = vec![self.own];
        for info in self.kernel.group(self.group.as_raw()).ok()? {
            looking.push(info.instance.pid);
            self.keep(&info);
        }
        looking.extend(before.iter().map(|instance| instance.pid));
        let mut looked = HashSet::new();
        while let Some(pid) = looking.pop() {
            if !looked.insert(pid) {
                continue;
            }
            for child in self.children(pid)? {
                if let Some(info) = self.kernel.info(child).ok()? {
                    self.keep(&info);
                }
                looking.push(child);
            }
        }
        let ended: Vec<Instance> = before
            .into_iter()
            .filter(|instance| self.kernel.ended(instance))
            .collect();
        let mut seen = self.seen();
        for instance in &ended {
            seen.remove(instance);
        }
        Some(!seen.is_empty())
    }

    /// Keep a process found running, never the supervisor itself.
    fn keep(&self, info: &Info) {
        if !info.zombie && info.instance.pid != self.own {
            self.seen().insert(info.instance);
        }
    }

    /// The children of `pid`: none for one that is gone, none when the
    /// kernel would not say of one still there.
    fn children(&self, pid: i32) -> Option<Vec<i32>> {
        match self.kernel.children(pid) {
            Ok(children) => Some(children),
            Err(_) => self.kernel.info(pid).ok()?.is_none().then(Vec::new),
        }
    }

    /// What ending the work signals, apart from its group: every process it
    /// has seen that is not known to have ended.
    fn targets(&self) -> Vec<Pid> {
        let seen = self.seen().clone();
        seen.iter()
            .filter(|instance| instance.pid != self.own && !self.kernel.ended(instance))
            .map(|instance| Pid::from_raw(instance.pid))
            .collect()
    }

    /// End every process this supervisor started, then itself: nothing it
    /// led outlives the authority it was started under. It never waits on
    /// a look.
    fn end(&self) -> ! {
        for pid in self.targets() {
            let _ = kill(pid, Signal::SIGKILL);
        }
        let _ = killpg(self.group, Signal::SIGKILL);
        // Killing the group killed this process too; should it not have, it
        // ends here all the same.
        std::process::exit(137)
    }
}

#[cfg(test)]
#[path = "supervise_tests.rs"]
mod tests;
