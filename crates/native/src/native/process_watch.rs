//! What the kernel tells of processes as it happens: that one started a
//! child, replaced its program, or ended. Any account may ask this of any
//! process, and nothing is polled.
use nix::{
    errno::Errno,
    sys::event::{EvFlags, EventFilter, FilterFlag, KEvent, Kqueue},
};
use std::time::Duration;

/// The wake's own name among the queue's events.
const WAKE: usize = 0;

/// The most notices one wait brings; the rest come with the next.
const NOTICES: usize = 32;

/// What a wait brought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// The watched parent started a process.
    Spawned(i32),
    /// The watched process replaced its program with another.
    Became(i32),
    /// The watched process ended.
    Ended(i32),
    /// [`Watch::wake`] was called.
    Woken,
}

/// The kernel's notices of the processes it was asked about.
pub struct Watch {
    queue: Kqueue,
}

impl Watch {
    /// A watch on nothing yet.
    ///
    /// # Errors
    ///
    /// Returns the kernel's refusal to make the queue.
    pub fn new() -> nix::Result<Self> {
        let queue = Kqueue::new()?;
        let wake = KEvent::new(
            WAKE,
            EventFilter::EVFILT_USER,
            EvFlags::EV_ADD | EvFlags::EV_CLEAR,
            FilterFlag::empty(),
            0,
            0,
        );
        queue.kevent(&[wake], &mut [], None)?;
        Ok(Self { queue })
    }

    /// End a wait now, from any thread.
    pub fn wake(&self) {
        let trigger = KEvent::new(
            WAKE,
            EventFilter::EVFILT_USER,
            EvFlags::empty(),
            FilterFlag::NOTE_TRIGGER,
            0,
            0,
        );
        // The queue lives as long as this watch, so nothing can refuse.
        let _ = self.queue.kevent(&[trigger], &mut [], None);
    }

    /// Tell each time `parent` starts a process, until forgotten.
    ///
    /// # Errors
    ///
    /// Returns the kernel's refusal, as for a process that is gone.
    pub fn spawns(&self, parent: i32) -> nix::Result<()> {
        self.set(
            parent,
            EvFlags::EV_ADD | EvFlags::EV_CLEAR,
            FilterFlag::NOTE_FORK,
        )
    }

    /// Tell when `pid` replaces its program and when it ends, until
    /// forgotten; its end forgets it.
    ///
    /// # Errors
    ///
    /// Returns the kernel's refusal, as for a process that is gone.
    pub fn process(&self, pid: i32) -> nix::Result<()> {
        self.set(
            pid,
            EvFlags::EV_ADD | EvFlags::EV_CLEAR,
            FilterFlag::NOTE_EXEC | FilterFlag::NOTE_EXIT,
        )
    }

    /// Tell nothing more of `pid`.
    pub fn forget(&self, pid: i32) {
        // One that ended is forgotten already.
        let _ = self.set(pid, EvFlags::EV_DELETE, FilterFlag::empty());
    }

    fn set(&self, pid: i32, flags: EvFlags, notes: FilterFlag) -> nix::Result<()> {
        let process = usize::try_from(pid).map_err(|_| Errno::ESRCH)?;
        let change = KEvent::new(process, EventFilter::EVFILT_PROC, flags, notes, 0, 0);
        self.queue.kevent(&[change], &mut [], None).map(drop)
    }

    /// What happened, in the kernel's order, waiting at most `longest` for
    /// the first of it; nothing when nothing did.
    ///
    /// # Errors
    ///
    /// Returns the kernel's refusal to wait, once `longest` has passed all
    /// the same: a queue that will not wait must not turn its caller into
    /// a busy loop.
    pub fn wait(&self, longest: Duration) -> nix::Result<Vec<Event>> {
        let blank = KEvent::new(
            WAKE,
            EventFilter::EVFILT_USER,
            EvFlags::empty(),
            FilterFlag::empty(),
            0,
            0,
        );
        let mut notices = [blank; NOTICES];
        let timeout = libc::timespec {
            tv_sec: libc::time_t::try_from(longest.as_secs()).unwrap_or(libc::time_t::MAX),
            tv_nsec: libc::c_long::from(longest.subsec_nanos()),
        };
        match self.queue.kevent(&[], &mut notices, Some(timeout)) {
            Ok(count) => Ok(notices.iter().take(count).flat_map(events).collect()),
            // A signal ended the wait early: nothing happened yet.
            Err(Errno::EINTR) => Ok(Vec::new()),
            Err(refusal) => {
                std::thread::sleep(longest);
                Err(refusal)
            }
        }
    }
}

/// What one notice says: a process can have replaced its program and ended
/// before the notice was read.
fn events(notice: &KEvent) -> Vec<Event> {
    match notice.filter() {
        Ok(EventFilter::EVFILT_USER) => vec![Event::Woken],
        Ok(EventFilter::EVFILT_PROC) => {
            let Ok(pid) = i32::try_from(notice.ident()) else {
                return Vec::new();
            };
            let notes = notice.fflags();
            [
                (FilterFlag::NOTE_FORK, Event::Spawned(pid)),
                (FilterFlag::NOTE_EXEC, Event::Became(pid)),
                (FilterFlag::NOTE_EXIT, Event::Ended(pid)),
            ]
            .into_iter()
            .filter(|(note, _)| notes.contains(*note))
            .map(|(_, event)| event)
            .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
#[path = "process_watch_tests.rs"]
mod tests;
