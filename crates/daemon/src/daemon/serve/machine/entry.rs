//! A machine desktop's daemon as its installed job starts it: enrolling
//! until a station is committed, then serving that station on the guest's
//! vsock port, and asking for Screen Recording in the VM's window.
use super::{
    FRAMING, Listener, Machine, OPENING, PAIRINGS,
    bounded::{BLOCKING, Bounded},
    listen, recording,
};
use crate::{
    daemon::{facts, serve::Daemon},
    machine::{
        input::{Input, Starter},
        key::GuestKey,
        mirror::Mirrors,
        pairing::Pairing,
        processes::{Processes, Supervising},
        stage::Stages,
        supervise::invocation::Supervisor,
        vault::{Keychain, Vault},
    },
};
use atlas_core::{
    desktop::DesktopId,
    error::{Code, Error, Result},
    files::{Token, private_folder},
    session::Console,
};
use atlas_session::{
    clock::Continuous,
    keys::own_key,
    pairing::{GuestFacts, GuestRecords, guest_pairs},
    secure::{GUEST_PORT, Purpose},
};
use std::{path::Path, path::PathBuf, sync::Arc};
use tokio::{sync::Semaphore, task::JoinSet};

/// What the machine's installed job names, after `--machine`: the desktop
/// it is, and the enrolment it pairs for. Root-owned, so no agent sets them.
#[derive(Clone, Debug)]
pub struct Named {
    pub desktop: DesktopId,
    pub enrolment: Token,
}

/// How long the shutdown waits for blocking work.
const SHUTDOWN: std::time::Duration = std::time::Duration::from_secs(5);

/// Serve as a machine desktop's daemon until it fails, keeping this thread
/// for the run loop macOS's frameworks need.
///
/// # Errors
///
/// Refuses any process but the installed job's, an administrator's
/// account, and fails when its records or its port cannot be had.
pub fn serve_blocking(named: Named) -> Result<()> {
    use objc2_core_foundation::{CFRunLoop, CFString};
    // Run-loop modes are compared by name; this is kCFRunLoopDefaultMode.
    let default_mode = CFString::from_static_str("kCFRunLoopDefaultMode");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let worker = std::thread::spawn(move || {
        let outcome = runtime.block_on(serve(named));
        runtime.shutdown_timeout(SHUTDOWN);
        outcome
    });
    while !worker.is_finished() {
        CFRunLoop::run_in_mode(Some(&default_mode), 0.25, false);
    }
    worker
        .join()
        .map_err(|_| Error::new(Code::Daemon, "The machine daemon stopped unexpectedly."))?
}

async fn serve(named: Named) -> Result<()> {
    crate::machine::launch::prove().await?;
    refuse_an_administrator().await?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::new(Code::State, "This account has no home folder."))?;
    let folder = home.join("Library/Application Support/Atlas/Machine");
    let records = folder.join("records");
    for private in [&folder, &records] {
        private_folder(private, Code::State)?;
    }
    let secret = folder.join("pairing-secret");
    let listener = listen::Guest::on(GUEST_PORT).map_err(|error| {
        Error::new(
            Code::Transport,
            "The guest's port could not be listened on.",
        )
        .with_source(error)
    })?;
    enroll(&listener, &named, &secret).await?;

    let paired = crate::machine::open(named.desktop, secret, records.clone()).await?;
    let supervising = supervising(&records)?;
    Console::Own.keep()?;
    let mut daemon = Daemon::new(
        facts::manager_name(),
        atlas_native::native::capture::main_display,
        facts::observed(Console::Own),
        atlas_native::native::session_info,
    );
    daemon.screen.console = Console::Own;
    let input_folder = folder.join("input");
    private_folder(&input_folder, Code::State)?;
    let input = Input::start(Starter::atlas(&input_folder)?, input_folder);
    let machine = Machine::new(
        daemon,
        paired,
        Arc::new(Continuous),
        supervising,
        Some(input),
        Some(Stages::new(records.join("open"))),
        Some(Mirrors::default()),
    );
    // Screen Recording, noted held and asked for in the VM's window, and
    // taken by restarting.
    let recording = recording::Own(machine.clone());
    let record = recording::Record::at(machine.paired.recording.clone());
    tokio::spawn(async move { recording::watch(&recording, &record).await });
    machine.serve(&listener).await
}

/// `machine::launch` trusts folders an administrator may write only
/// because a machine's account is none: one that is, or cannot be told,
/// serves nothing.
async fn refuse_an_administrator() -> Result<()> {
    let uid = nix::unistd::getuid();
    let administrator = tokio::task::spawn_blocking(move || {
        let account = nix::unistd::User::from_uid(uid).ok().flatten()?;
        atlas_core::account::administrator(&account.name)
    })
    .await
    .ok()
    .flatten();
    match administrator {
        Some(false) => Ok(()),
        Some(true) => Err(Error::new(
            Code::Identity,
            "A machine desktop's account must not be an administrator.",
        )),
        None => Err(Error::new(
            Code::Identity,
            "Whether this account is an administrator cannot be told.",
        )),
    }
}

/// Pair for `named`'s enrolment, unless a station is committed for it
/// already: pairing connections only, framed and tried a few at once, so
/// no connection that stalls holds up another.
async fn enroll(listener: &listen::Guest, named: &Named, secret: &Path) -> Result<()> {
    let vault = Keychain::atlas();
    let records: Arc<dyn GuestRecords> = Arc::new(Pairing::new(vault.clone(), secret));
    match committed(&records).await? {
        Some(enrolment) if enrolment == named.enrolment => return Ok(()),
        Some(_) => {
            return Err(Error::new(
                Code::Identity,
                "This desktop is paired for another enrolment than its job names.",
            ));
        }
        None => {}
    }
    let keys = tokio::task::spawn_blocking(move || own_key(&GuestKey::new(vault)))
        .await
        .map_err(|_| Error::new(Code::State, "The guest's key could not be read."))??;
    let facts = GuestFacts {
        uid: nix::unistd::getuid().as_raw(),
        boot: atlas_core::process::boot()?,
    };
    let (keys, facts, named) = (Arc::new(keys), Arc::new(facts), Arc::new(named.clone()));
    // Its records' reads go on past any timeout: bounded while they run.
    let records: Arc<dyn GuestRecords> = Arc::new(Bounded {
        records,
        permits: Arc::new(Semaphore::new(BLOCKING)),
    });
    let (framing, pairings) = (
        Arc::new(Semaphore::new(FRAMING)),
        Arc::new(Semaphore::new(PAIRINGS)),
    );
    let mut attempts = JoinSet::new();
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let framed = accepted.map_err(|error| {
                    Error::new(Code::Transport, "The guest stopped taking connections.")
                        .with_source(error)
                })?;
                let Ok(saying) = framing.clone().try_acquire_owned() else {
                    continue;
                };
                let (pairings, keys, facts, named, records) = (
                    pairings.clone(),
                    keys.clone(),
                    facts.clone(),
                    named.clone(),
                    records.clone(),
                );
                attempts.spawn(async move {
                    let framed = tokio::time::timeout(OPENING, framed).await;
                    drop(saying);
                    let Ok(Ok((Purpose::Pairing, raw))) = framed else {
                        return false;
                    };
                    let Ok(_room) = pairings.try_acquire_owned() else {
                        return false;
                    };
                    let pairing =
                        guest_pairs(raw, &keys, &named.desktop, &named.enrolment, &facts, &records);
                    if matches!(tokio::time::timeout(OPENING, pairing).await, Ok(Ok(_))) {
                        return true;
                    }
                    // Committed though its acknowledgment failed: the station
                    // finishes it once paired, as any lost acknowledgment.
                    let committing = tokio::time::timeout(OPENING, committed(&records));
                    matches!(committing.await, Ok(Ok(Some(_))))
                });
            }
            Some(attempt) = attempts.join_next() => {
                if matches!(attempt, Ok(true)) {
                    return Ok(());
                }
            }
        }
    }
}

/// The enrolment `records` commit this guest to, if any.
async fn committed(records: &Arc<dyn GuestRecords>) -> Result<Option<Token>> {
    let reading = records.clone();
    let committed = tokio::task::spawn_blocking(move || reading.committed())
        .await
        .map_err(|_| Error::new(Code::State, "The pairing could not be read."))??;
    Ok(committed.map(|(_, enrolment)| enrolment))
}

/// The supervisors' store and proofs in `records`, the proofs' folder
/// private: a supervisor that cannot leave its proof is `UNSURE`.
fn supervising(records: &Path) -> Result<Supervising> {
    let folder = records.join("supervisors");
    let proofs = folder.join("proofs");
    for private in [&folder, &proofs] {
        private_folder(private, Code::State)?;
    }
    let vault: Arc<dyn Vault> = Arc::new(Keychain::atlas());
    Ok(Supervising {
        supervisor: Supervisor::atlas()?,
        proofs,
        kept: Arc::new(Processes::new(folder.join("processes.json"), vault)),
        worker: std::env::current_exe()?,
    })
}
