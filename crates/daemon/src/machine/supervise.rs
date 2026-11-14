//! What a machine guest's commands and jobs run under: a supervisor that
//! leads their process group and ends it the moment the daemon that
//! started it is gone or stops renewing its lease. The daemon's own
//! watchdog cannot stop work once the daemon is gone, and a new daemon may
//! never come; the supervisor never waits for one.
//!
//! Its standard input is the lifeline, a Unix socket its own alone: the
//! daemon writes [`RENEW`] while the generation the work belongs to is
//! live, and the supervisor says back how the program ended. The socket
//! closing, a lease gone unrenewed, or anything else on it, ends every
//! process the supervisor started: its group, and every descendant it has
//! seen leave the group. It goes on supervising until all of them are gone,
//! the program itself ending first or not.
//!
//! macOS gives a process no containment that survives both leaving its
//! group and losing its parent: a descendant that does both between two
//! looks is not seen.
use atlas_core::process::{Info, Instance};
use atlas_session::clock::{Clock, Continuous};
use nix::{
    sys::signal::{Signal, kill, killpg},
    unistd::{Pid, getpgrp},
};
use std::{
    collections::HashSet,
    ffi::OsString,
    io::{ErrorKind, Read, Write},
    os::{
        fd::AsFd,
        unix::{net::UnixStream, process::CommandExt},
    },
    process::{Command, Stdio},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

pub mod invocation;

use invocation::Invocation;

/// What the daemon writes to renew the lease.
pub const RENEW: u8 = b'.';

/// How often the supervisor looks, at most.
const LOOK: Duration = Duration::from_millis(100);

/// How long the work has to end once the supervisor is asked to end it,
/// before it is made to: within the daemon's own grace.
const TERMINATING: Duration = Duration::from_millis(1500);

/// The supervisor's command: run what `words` give under the lifeline on
/// standard input, the program's output on standard error. The exit code is 0 once everything it started has
/// ended, and 127 when the words were not an invocation or nothing could
/// be started; the program's own end goes back on the lifeline.
pub fn start(words: &[OsString]) -> u8 {
    let Some(invocation) = Invocation::parse(words) else {
        return 127;
    };
    let Ok(lifeline) = std::io::stdin().as_fd().try_clone_to_owned() else {
        return 127;
    };
    supervise(&invocation, UnixStream::from(lifeline));
    0
}

/// Run `invocation`'s program in this process's own group, under
/// `lifeline`, ending everything it started when no renewal comes within
/// the lease on the continuous clock. Once the program ends, its end goes
/// back on the lifeline; this returns once all it left running has ended
/// too. Anything that goes wrong ends the work.
pub fn supervise(invocation: &Invocation, lifeline: UnixStream) {
    let work = Arc::new(Work::new(Live, getpgrp()));
    // Asked to end, before anything could be started that it must end.
    if !on_terminate(Arc::clone(&work)) {
        work.end();
    }
    // The lease runs from before anything starts, and a clock that cannot
    // be read starts nothing.
    let lease = Arc::new(Lease::start(Continuous, invocation.lease).unwrap_or_else(|| work.end()));
    let mut telling = lifeline.try_clone().unwrap_or_else(|_| work.end());
    if !listen(lifeline, Arc::clone(&lease)) || !watch(Arc::clone(&lease), Arc::clone(&work)) {
        work.end();
    }
    // The program's output, both streams, is this process's standard
    // error: its standard output is not the program's to write on.
    let output = std::io::stderr()
        .as_fd()
        .try_clone_to_owned()
        .unwrap_or_else(|_| work.end());
    let mut command = Command::new(&invocation.program);
    command
        .arg0(&invocation.argv0)
        .args(&invocation.arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::from(output));
    // Checked again once nothing else may start: a lease that ran out while
    // waiting starts nothing.
    let Ok(Some(mut child)) = atlas_core::spawn::spawn_if(&mut command, || lease.live()) else {
        work.end();
    };
    // The program has the output now: this process keeps none of it open.
    drop(command);
    let _ = let_go_of_output();
    let mut told = false;
    loop {
        if !told && let Some(status) = child.try_wait().unwrap_or_else(|_| work.end()) {
            // Said once; a daemon that no longer listens changes nothing,
            // as only the lifeline says whether the work lives on.
            let _ = telling.write_all(&invocation::told(status));
            told = true;
        }
        let left = work.look().unwrap_or_else(|| work.end());
        if told && !left {
            return;
        }
        std::thread::sleep(LOOK);
    }
}

/// Point this process's standard error, the program's output, at nothing.
fn let_go_of_output() -> std::io::Result<()> {
    let nothing = std::fs::OpenOptions::new().write(true).open("/dev/null")?;
    nix::unistd::dup2_stderr(&nothing).map_err(std::io::Error::from)
}

/// Take SIGTERM on a thread of its own: pass it on, once, to everything
/// the supervisor started, and end them all once they have had a grace. A
/// handler, not a mask, so the program starts with the default action.
/// False when it cannot be set up.
fn on_terminate<K: Kernel + 'static>(work: Arc<Work<K>>) -> bool {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .build()
    else {
        return false;
    };
    let terms = {
        let _inside = runtime.enter();
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
    };
    let Ok(mut terms) = terms else {
        return false;
    };
    std::thread::Builder::new()
        .spawn(move || {
            runtime.block_on(terms.recv());
            work.terminate();
            std::thread::sleep(TERMINATING);
            work.end();
        })
        .is_ok()
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

    /// Ask everything it started to end, once each: its group, then every
    /// process it has seen that left the group.
    fn terminate(&self) {
        let _ = killpg(self.group, Signal::SIGTERM);
        for pid in self.targets() {
            let outside = self
                .kernel
                .info(pid.as_raw())
                .ok()
                .flatten()
                .is_some_and(|info| info.pgid != self.group.as_raw());
            if outside {
                let _ = kill(pid, Signal::SIGTERM);
            }
        }
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
