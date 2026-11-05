//! Keeping the helper's apps silent on the person's speakers. A private Core
//! Audio process tap mutes the audio processes of this account only, inside a
//! private aggregate device whose running IOProc drops every buffer: the tap
//! has no effect without it. Nothing is ever read from the tap, no device's
//! volume or mute is touched, and no tap is exclusive or names bundles.
//!
//! macOS lets the tap mute only once the person allowed Atlas to record the
//! helper's audio. Asking shows a prompt only while the helper is on screen;
//! an unanswered prompt off screen becomes a lasting denial. So [`access`]
//! only reads the answer, and nothing here starts IO before it is yes.
#![allow(
    unsafe_code,
    reason = "FFI: Core Audio taps, aggregate devices and IOProcs, and TCC's preflight, have no safe binding"
)]
use atlas_core::error::{Code, Error, Result};
use block2::{DynBlock, RcBlock};
use dispatch2::{DispatchQueue, DispatchRetained};
use objc2::{
    AnyThread,
    rc::Retained,
    runtime::{AnyClass, NSObjectProtocol},
    sel,
};
use objc2_core_audio::{
    AudioDeviceCreateIOProcIDWithBlock, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID,
    AudioDeviceStart, AudioDeviceStop, AudioHardwareCreateAggregateDevice,
    AudioHardwareDestroyAggregateDevice, AudioObjectAddPropertyListenerBlock,
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectPropertySelector,
    AudioObjectRemovePropertyListenerBlock, CATapDescription, CATapMuteBehavior,
    kAudioAggregateDeviceIsPrivateKey, kAudioAggregateDeviceNameKey,
    kAudioAggregateDeviceTapAutoStartKey, kAudioAggregateDeviceTapListKey,
    kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceUID, kAudioHardwareBadObjectError,
    kAudioHardwarePropertyProcessObjectList, kAudioHardwarePropertyServiceRestarted,
    kAudioHardwareUnknownPropertyError, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, kAudioProcessPropertyPID,
    kAudioSubTapUIDKey, kAudioTapPropertyUID,
};
use objc2_core_audio_types::{AudioBufferList, AudioTimeStamp};
use objc2_core_foundation::{CFArray, CFBoolean, CFDictionary, CFRetained, CFString, CFType};
use objc2_foundation::{NSArray, NSNumber, NSUUID};
use std::{
    cell::Cell,
    ffi::{CStr, c_int, c_void},
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

/// Whether the person allowed Atlas to record the helper's audio, which
/// muting it needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Granted,
    Denied,
    /// Not asked yet, or not readable: muting waits until it is yes.
    Undetermined,
}

const TCC: &CStr = c"/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC";
const AUDIO_CAPTURE: &str = "kTCCServiceAudioCapture";

/// A function of macOS's privacy framework, resolved at run time.
fn tcc(symbol: &CStr) -> Option<NonNull<c_void>> {
    // SAFETY: a NUL-terminated path; the handle stays open for the process.
    let handle = unsafe { libc::dlopen(TCC.as_ptr(), libc::RTLD_LAZY) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: a live handle and a NUL-terminated symbol name.
    NonNull::new(unsafe { libc::dlsym(handle, symbol.as_ptr()) })
}

/// Whether this process may record the helper's audio, read without ever
/// asking the person.
pub fn access() -> Access {
    type Preflight = unsafe extern "C" fn(*const CFString, *const c_void) -> c_int;

    let Some(function) = tcc(c"TCCAccessPreflight") else {
        return Access::Undetermined;
    };
    // SAFETY: TCCAccessPreflight takes a service name and optional options
    // and returns its answer as an int (0 allowed, 1 denied, else not
    // answered yet).
    let preflight = unsafe { std::mem::transmute::<*mut c_void, Preflight>(function.as_ptr()) };
    let service = CFString::from_static_str(AUDIO_CAPTURE);
    // SAFETY: a live service name and no options; preflight only reads.
    match unsafe { preflight(&*service, std::ptr::null()) } {
        0 => Access::Granted,
        1 => Access::Denied,
        _ => Access::Undetermined,
    }
}

/// Ask the person, with macOS's prompt, to let Atlas record the helper's
/// audio; `answered` runs once they did. Only ask while the helper is on
/// screen: off screen the prompt goes unseen and macOS records a denial.
/// Whether the request could be made.
pub fn request_access(answered: impl Fn() + Send + Sync + 'static) -> bool {
    type Request = unsafe extern "C" fn(*const CFString, *const c_void, *mut DynBlock<dyn Fn(u8)>);

    let Some(function) = tcc(c"TCCAccessRequest") else {
        return false;
    };
    // SAFETY: TCCAccessRequest takes a service name, optional options and a
    // block told whether access was granted, and returns nothing.
    let request = unsafe { std::mem::transmute::<*mut c_void, Request>(function.as_ptr()) };
    let service = CFString::from_static_str(AUDIO_CAPTURE);
    let block = RcBlock::new(move |_granted: u8| answered());
    // SAFETY: a live service name, no options and a heap block TCC copies
    // for as long as it needs it.
    unsafe { request(&*service, std::ptr::null(), RcBlock::as_ptr(&block)) };
    true
}

/// The process objects among `objects` that belong to `uid`: each object's
/// process must have that effective and real user ID, and the object must
/// still name the same process after its owner was read. A process that is
/// gone, or someone else's, is left out, so it plays rather than risk
/// muting someone else. `pid_of` and `owner_of` say none for a process or
/// object that is gone.
///
/// # Errors
///
/// Returns the first lookup that failed: the helper's processes are then
/// unknown, never fewer.
pub fn owned_by(
    objects: &[AudioObjectID],
    uid: u32,
    pid_of: impl Fn(AudioObjectID) -> Result<Option<i32>>,
    owner_of: impl Fn(i32) -> Result<Option<u32>>,
) -> Result<Vec<AudioObjectID>> {
    let mut owned = Vec::new();
    for &object in objects {
        let Some(pid) = pid_of(object)?.filter(|pid| *pid > 0) else {
            continue;
        };
        if owner_of(pid)? == Some(uid) && pid_of(object)? == Some(pid) {
            owned.push(object);
        }
    }
    owned.sort_unstable();
    owned.dedup();
    Ok(owned)
}

/// Whether an object Atlas made is still its own, by the UID it was made
/// with: after Core Audio restarts, its ID may name anyone's object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ownership {
    Ours,
    /// Gone, or someone else's now: never to be destroyed.
    Gone,
    /// It could not be read: ask again later.
    Unknown,
}

/// What a mute is built from: the audio processes there are now, and a way
/// to build and end the mute itself. Core Audio in Atlas, a fake in tests.
///
/// Every restart of Core Audio starts a new generation. A listing from an
/// older generation is never built from, and a mute from one is ended
/// through [`Backend::tear_down`], which destroys only what is still ours.
pub trait Backend {
    /// One built mute, or what was made of one before building it failed.
    type Mute;

    /// Core Audio's generation now: how many times it restarted.
    fn generation(&self) -> u64;

    /// The helper's audio process objects now, sorted and without repeats.
    ///
    /// # Errors
    ///
    /// Returns the failure to list them, which is never an empty list.
    fn objects(&self) -> Result<Vec<AudioObjectID>>;

    /// Whether the person allows Atlas to record the helper's audio now,
    /// read afresh without asking: a mute starts IO only when it is yes.
    fn access(&self) -> Access;

    /// Whether each of `objects` still belongs to a helper process, read
    /// afresh.
    ///
    /// # Errors
    ///
    /// Returns the failure to read one.
    fn still_ours(&self, objects: &[AudioObjectID]) -> Result<bool>;

    /// Make the mute of exactly `objects`, its IO not started.
    ///
    /// # Errors
    ///
    /// Returns Core Audio's refusal, with what was made before it, if
    /// anything, for [`Backend::tear_down`].
    fn build(
        &self,
        objects: &[AudioObjectID],
    ) -> std::result::Result<Self::Mute, Box<Unbuilt<Self::Mute>>>;

    /// Start `mute`'s IO, which makes it mute, only while `generation` is
    /// still Core Audio's.
    ///
    /// # Errors
    ///
    /// Returns Core Audio's refusal, or that it restarted.
    fn start(&self, mute: &mut Self::Mute, generation: u64) -> Result<()>;

    /// End `mute`: destroy each of its objects that is still ours, and let
    /// go of those that are gone. Gives back what could not be checked.
    fn tear_down(&self, mute: Self::Mute) -> Option<Self::Mute>;
}

/// A build that failed, and what it made before failing.
#[derive(Debug)]
pub struct Unbuilt<M> {
    pub error: Error,
    pub made: Option<M>,
}

/// Why a mute was not brought up to date.
#[derive(Debug)]
pub enum Refusal {
    /// The person does not (or no longer) allow it: the mute was ended.
    Access(Access),
    /// Core Audio failed; a mute already running stays.
    Failed(Error),
    /// Core Audio restarted, or a process listed changed owner, before IO
    /// started: what was built was ended, and the processes must be listed
    /// again.
    Stale,
}

/// A built mute: what it mutes, and the generation it was made in.
struct Built<M> {
    members: Vec<AudioObjectID>,
    mute: M,
    generation: u64,
}

/// The longest wait before ending a mute is tried again.
const LONGEST_RETRY: Duration = Duration::from_secs(60);

/// How long after its `tries`-th try ending a mute is tried again: a
/// second, doubling to [`LONGEST_RETRY`]. It is never given up.
fn retry_in(tries: u32) -> Duration {
    Duration::from_secs(1u64 << tries.saturating_sub(1).min(6)).min(LONGEST_RETRY)
}

/// A mute not yet fully ended, and when to try again.
struct Leftover<M> {
    mute: M,
    tries: u32,
    next: Instant,
}

/// One mute over the helper's processes, rebuilt whole when they change:
/// Core Audio refuses to change a tap's processes in place. A new mute is
/// built before the one it replaces ends, so nothing plays in between.
/// Nothing Atlas made is let go until it is destroyed or proven gone.
pub struct Engine<B: Backend> {
    backend: B,
    current: Option<Built<B::Mute>>,
    /// Mutes not yet fully ended: a stage failed, or its object could not
    /// be checked. They are tried again until they end.
    leftovers: Vec<Leftover<B::Mute>>,
}

impl<B: Backend> Engine<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            current: None,
            leftovers: Vec::new(),
        }
    }

    /// Mute the helper's processes as they are now. Nothing changes while
    /// they are the ones muted already. The permission is read first, as it
    /// may block: without a yes, the mute ends. Then the processes are
    /// listed and the mute built at once, and right before its IO starts,
    /// each process is checked to still be the helper's and Core Audio not
    /// to have restarted.
    ///
    /// # Errors
    ///
    /// Returns [`Refusal::Access`] when the permission is not a yes, the mute
    /// then ended; [`Refusal::Failed`] when Core Audio could not list the
    /// processes or build the mute, the previous one kept; and
    /// [`Refusal::Stale`] when Core Audio restarted, or a process changed,
    /// before IO started.
    pub fn refresh(&mut self) -> std::result::Result<(), Refusal> {
        self.sweep(Instant::now());
        let generation = self.backend.generation();
        self.end_older(generation);
        match self.backend.access() {
            Access::Granted => {}
            access => {
                self.stop();
                return Err(Refusal::Access(access));
            }
        }
        // Core Audio may have restarted while the permission was read.
        if self.backend.generation() != generation {
            return Err(Refusal::Stale);
        }

        let objects = self.backend.objects().map_err(Refusal::Failed)?;
        if self
            .current
            .as_ref()
            .is_some_and(|built| built.members == objects)
        {
            return Ok(());
        }

        if objects.is_empty() {
            self.stop();
            return Ok(());
        }

        let mut mute = match self.backend.build(&objects) {
            Ok(mute) => mute,
            Err(unbuilt) => {
                let Unbuilt { error, made } = *unbuilt;
                if let Some(made) = made {
                    self.end(made);
                }
                return Err(Refusal::Failed(error));
            }
        };
        // Right before IO starts: a restart whose notice is late, or an ID
        // now naming someone else's process, ends the mute unstarted.
        let fresh = if self.backend.generation() == generation {
            self.backend.still_ours(&objects).map_err(Refusal::Failed)
        } else {
            Ok(false)
        };
        let started = match fresh {
            Ok(true) => self
                .backend
                .start(&mut mute, generation)
                .map_err(Refusal::Failed),
            Ok(false) => Err(Refusal::Stale),
            Err(refusal) => Err(refusal),
        };
        if let Err(refusal) = started {
            self.end(mute);
            return Err(refusal);
        }
        let replaced = self.current.replace(Built {
            members: objects,
            mute,
            generation,
        });
        if let Some(old) = replaced {
            self.end(old.mute);
        }
        Ok(())
    }

    /// Whether a helper process is listed now that the mute does not cover
    /// yet: a new app, whose first sound would play until it does.
    ///
    /// # Errors
    ///
    /// Returns the failure to list the processes.
    pub fn uncovered(&self) -> Result<bool> {
        let objects = self.backend.objects()?;
        let members = self.current.as_ref().map(|built| &built.members);
        Ok(objects
            .iter()
            .any(|object| members.is_none_or(|members| !members.contains(object))))
    }

    /// Whether a mute is running.
    pub fn running(&self) -> bool {
        self.current.is_some()
    }

    /// End a mute made before `generation`: Core Audio restarted since, so
    /// whatever of it is still ours mutes from a listing that may be stale.
    pub fn end_older(&mut self, generation: u64) {
        if self
            .current
            .as_ref()
            .is_some_and(|built| built.generation < generation)
            && let Some(old) = self.current.take()
        {
            self.end(old.mute);
        }
    }

    /// End the mute, so the helper's apps play again.
    pub fn stop(&mut self) {
        self.sweep(Instant::now());
        if let Some(built) = self.current.take() {
            self.end(built.mute);
        }
    }

    /// End `mute`, keeping what is left of it to try again.
    fn end(&mut self, mute: B::Mute) {
        if let Some(mute) = self.backend.tear_down(mute) {
            self.leftovers.push(Leftover {
                mute,
                tries: 1,
                next: Instant::now() + retry_in(1),
            });
        }
    }

    /// Try again, as of `now`, to end the mutes whose time came.
    pub fn sweep(&mut self, now: Instant) {
        for leftover in std::mem::take(&mut self.leftovers) {
            if now < leftover.next {
                self.leftovers.push(leftover);
            } else if let Some(mute) = self.backend.tear_down(leftover.mute) {
                let tries = leftover.tries + 1;
                self.leftovers.push(Leftover {
                    mute,
                    tries,
                    next: now + retry_in(tries),
                });
            }
        }
    }

    /// When a mute not yet fully ended is next tried; none when every one
    /// ended.
    pub fn next_retry(&self) -> Option<Instant> {
        self.leftovers.iter().map(|leftover| leftover.next).min()
    }
}

impl<B: Backend> Drop for Engine<B> {
    fn drop(&mut self) {
        self.stop();
    }
}

type CreateTap = unsafe extern "C" fn(*const CATapDescription, *mut AudioObjectID) -> i32;
type DestroyTap = unsafe extern "C" fn(AudioObjectID) -> i32;
type IoBlock = RcBlock<
    dyn Fn(
        NonNull<AudioTimeStamp>,
        NonNull<AudioBufferList>,
        NonNull<AudioTimeStamp>,
        NonNull<AudioBufferList>,
        NonNull<AudioTimeStamp>,
    ),
>;

/// Core Audio's process taps, for the helper account `uid`.
pub struct CoreAudio {
    uid: u32,
    /// Core Audio's restarts, counted by whoever hears of them.
    generation: Arc<AtomicU64>,
    create_tap: CreateTap,
    destroy_tap: DestroyTap,
    /// Mutes built so far, which keeps each aggregate device's UID unique.
    built: Cell<u64>,
}

/// A mute, or what was made of one: the tap, the aggregate device holding
/// it, and the IOProc running on that. Each object keeps the UID it was
/// made with, which proves its ID still names it.
pub struct Tap {
    tap: Option<(AudioObjectID, String)>,
    device: Option<(AudioObjectID, String)>,
    io: Option<(AudioDeviceIOProcID, IoBlock)>,
    /// Whether the IOProc runs.
    running: bool,
}

/// A Core Audio function present only on macOS 14.2 and later, resolved at
/// run time so an older macOS still starts Atlas.
fn core_audio(symbol: &CStr) -> Option<NonNull<c_void>> {
    // SAFETY: the default search handle and a NUL-terminated symbol name.
    NonNull::new(unsafe { libc::dlsym(libc::RTLD_DEFAULT, symbol.as_ptr()) })
}

impl CoreAudio {
    /// Process taps for `uid`, whose generation is the count of Core Audio
    /// restarts in `generation`; none on a macOS before 14.2, which has none.
    pub fn new(uid: u32, generation: Arc<AtomicU64>) -> Option<Self> {
        AnyClass::get(c"CATapDescription")?;
        let create = core_audio(c"AudioHardwareCreateProcessTap")?;
        let destroy = core_audio(c"AudioHardwareDestroyProcessTap")?;
        Some(Self {
            uid,
            generation,
            // SAFETY: AudioHardwareCreateProcessTap takes a tap description
            // and writes the new tap's ID, returning an OSStatus.
            create_tap: unsafe { std::mem::transmute::<*mut c_void, CreateTap>(create.as_ptr()) },
            // SAFETY: AudioHardwareDestroyProcessTap takes a tap's ID and
            // returns an OSStatus.
            destroy_tap: unsafe {
                std::mem::transmute::<*mut c_void, DestroyTap>(destroy.as_ptr())
            },
            built: Cell::new(0),
        })
    }

    /// Make the mute of `objects`: the tap, with a UID chosen before it is
    /// made, then the device holding it, then its IOProc. A step that fails
    /// gives back what the steps before it made.
    fn create(&self, objects: &[AudioObjectID]) -> std::result::Result<Tap, Box<Unbuilt<Tap>>> {
        let mut made = Tap {
            tap: None,
            device: None,
            io: None,
            running: false,
        };
        let failed = |error, made: Tap| {
            Err(Box::new(Unbuilt {
                error,
                made: made.tap.is_some().then_some(made),
            }))
        };

        let description = description(objects);
        let uid = NSUUID::new();
        // SAFETY: a property setter of a description this function owns.
        unsafe { description.setUUID(&uid) };
        let tap_uid = uid.UUIDString().to_string();
        let mut tap = 0;
        // SAFETY: a live description and a place for the new tap's ID.
        let status = unsafe { (self.create_tap)(Retained::as_ptr(&description), &mut tap) };
        if let Err(error) = refused("create the tap", status) {
            return failed(error, made);
        }
        made.tap = Some((tap, tap_uid.clone()));

        match self.aggregate(&tap_uid) {
            Ok(device) => made.device = Some(device),
            Err(error) => return failed(error, made),
        }
        let device = made.device.as_ref().map_or(0, |(device, _)| *device);
        match io_proc(device) {
            Ok(io) => made.io = Some(io),
            Err(error) => return failed(error, made),
        }
        Ok(made)
    }

    /// The helper's processes among `objects`, read afresh.
    fn owned(&self, objects: &[AudioObjectID]) -> Result<Vec<AudioObjectID>> {
        // A process whose real and effective owners differ is nobody's here.
        let owner_of = |pid| {
            Ok(atlas_core::process::info(pid)?
                .filter(|info| !info.zombie)
                .map(|info| {
                    if info.ruid == info.uid {
                        info.uid
                    } else {
                        u32::MAX
                    }
                }))
        };
        owned_by(objects, self.uid, process_pid, owner_of)
    }

    /// A private aggregate device holding the tap `tap_uid` names, which
    /// starts it, and the device's UID.
    fn aggregate(&self, tap_uid: &str) -> Result<(AudioObjectID, String)> {
        let built = self.built.get() + 1;
        self.built.set(built);

        let key = |name: &CStr| CFString::from_str(&name.to_string_lossy());
        let entry = CFDictionary::<CFString, CFString>::from_slices(
            &[&*key(kAudioSubTapUIDKey)],
            &[&*CFString::from_str(tap_uid)],
        );
        let taps = CFArray::from_objects(&[&*entry]);
        let name = CFString::from_static_str("Atlas helper mute");
        let uid = CFString::from_str(&format!(
            "com.insanearts.atlas.mute.{}.{built}",
            std::process::id()
        ));
        let yes = CFBoolean::new(true);
        let keys = [
            key(kAudioAggregateDeviceNameKey),
            key(kAudioAggregateDeviceUIDKey),
            key(kAudioAggregateDeviceIsPrivateKey),
            key(kAudioAggregateDeviceTapAutoStartKey),
            key(kAudioAggregateDeviceTapListKey),
        ];
        let values: [&CFType; 5] = [&name, &uid, yes, yes, &taps];
        let keys: Vec<&CFString> = keys.iter().map(|key| &**key).collect();
        let description = CFDictionary::<CFString, CFType>::from_slices(&keys, &values);

        let mut device = 0;
        // SAFETY: a dictionary of the documented aggregate keys and a place
        // for the new device's ID.
        let status = unsafe {
            AudioHardwareCreateAggregateDevice(description.as_opaque(), NonNull::from(&mut device))
        };
        refused("create the device", status)?;
        Ok((device, uid.to_string()))
    }
}

/// A tap description of exactly `objects`: a private stereo mixdown that
/// mutes them. Never exclusive, never of bundles, and never restoring a
/// process that ended by its bundle: a later app of that bundle may be the
/// person's.
pub fn description(objects: &[AudioObjectID]) -> Retained<CATapDescription> {
    let numbers: Vec<_> = objects
        .iter()
        .map(|object| NSNumber::new_u32(*object))
        .collect();
    let processes = NSArray::from_retained_slice(&numbers);
    // SAFETY: an array of process object IDs as numbers, as the initializer
    // takes.
    let description = unsafe {
        CATapDescription::initStereoMixdownOfProcesses(CATapDescription::alloc(), &processes)
    };
    // SAFETY: property setters of a description this function owns.
    unsafe {
        description.setPrivate(true);
        description.setMuteBehavior(CATapMuteBehavior::Muted);
    }
    // Restoring is on by default where macOS has it.
    if description.respondsToSelector(sel!(setProcessRestoreEnabled:)) {
        // SAFETY: a property setter this macOS answers to.
        unsafe { description.setProcessRestoreEnabled(false) };
    }
    description
}

/// Add an IOProc to `device` that drops everything it is given: a tap
/// mutes only while its device runs one.
fn io_proc(device: AudioObjectID) -> Result<(AudioDeviceIOProcID, IoBlock)> {
    let io: IoBlock = RcBlock::new(
        |_: NonNull<AudioTimeStamp>,
         _: NonNull<AudioBufferList>,
         _: NonNull<AudioTimeStamp>,
         _: NonNull<AudioBufferList>,
         _: NonNull<AudioTimeStamp>| {},
    );
    let mut proc_id: AudioDeviceIOProcID = None;
    // SAFETY: a place for the IOProc's ID, a live device and a block Core
    // Audio copies; no queue runs it on the device's own thread.
    let status = unsafe {
        AudioDeviceCreateIOProcIDWithBlock(
            NonNull::from(&mut proc_id),
            device,
            None,
            RcBlock::as_ptr(&io),
        )
    };
    refused("add the IOProc", status)?;
    Ok((proc_id, io))
}

impl Backend for CoreAudio {
    type Mute = Tap;

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn objects(&self) -> Result<Vec<AudioObjectID>> {
        self.owned(&process_objects()?)
    }

    fn still_ours(&self, objects: &[AudioObjectID]) -> Result<bool> {
        Ok(self.owned(objects)? == objects)
    }

    fn access(&self) -> Access {
        access()
    }

    fn build(&self, objects: &[AudioObjectID]) -> std::result::Result<Tap, Box<Unbuilt<Tap>>> {
        self.create(objects)
    }

    fn start(&self, mute: &mut Tap, generation: u64) -> Result<()> {
        if self.generation() != generation {
            return Err(Error::new(
                Code::Daemon,
                "Core Audio restarted while Atlas muted the helper",
            ));
        }
        let (Some((device, _)), Some((proc_id, _))) = (&mute.device, &mute.io) else {
            return Err(failed("start the device", 0));
        };
        // SAFETY: the device and the IOProc made on it, by this backend.
        let status = unsafe { AudioDeviceStart(*device, *proc_id) };
        refused("start the device", status)?;
        mute.running = true;
        Ok(())
    }

    fn tear_down(&self, mut mute: Tap) -> Option<Tap> {
        loop {
            match self.advance(&mut mute)? {
                Stage::Done | Stage::Gone => {}
                Stage::Pending => return Some(mute),
            }
        }
    }
}

/// What came of one teardown stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// It ran and Core Audio confirmed it.
    Done,
    /// Its object is gone or no longer ours: nothing is left to do there.
    Gone,
    /// It failed, or its object could not be checked: try again later.
    Pending,
}

/// One destructive teardown stage: `call` runs only when `owned` proves its
/// object ours right before, and counts only when Core Audio says it worked
/// or the object is then proven gone.
pub fn stage(owned: impl Fn() -> Ownership, call: impl FnOnce() -> i32) -> Stage {
    match owned() {
        Ownership::Ours => {
            if call() == 0 {
                Stage::Done
            } else if owned() == Ownership::Gone {
                Stage::Gone
            } else {
                Stage::Pending
            }
        }
        Ownership::Gone => Stage::Gone,
        Ownership::Unknown => Stage::Pending,
    }
}

impl CoreAudio {
    /// Take the next stage of ending `mute`, in order: stop its IO, destroy
    /// its IOProc, its device, then its tap. None when nothing is left.
    fn advance(&self, mute: &mut Tap) -> Option<Stage> {
        if let Some((device, uid)) = mute.device.clone() {
            let owned = || ownership(device, kAudioDevicePropertyDeviceUID, &uid);
            let proc_id = mute.io.as_ref().map(|(proc_id, _)| *proc_id);
            let stage = match proc_id {
                Some(proc_id) if mute.running => {
                    // SAFETY: a device this backend made, its UID just
                    // checked, and the IOProc made on it.
                    let stage = stage(owned, || unsafe { AudioDeviceStop(device, proc_id) });
                    if stage == Stage::Done {
                        mute.running = false;
                    }
                    stage
                }
                Some(proc_id) => {
                    // SAFETY: as for stopping it.
                    let stage = stage(owned, || unsafe {
                        AudioDeviceDestroyIOProcID(device, proc_id)
                    });
                    if stage == Stage::Done {
                        mute.io = None;
                    }
                    stage
                }
                None => {
                    // SAFETY: a device this backend made, its UID just checked.
                    let stage = stage(owned, || unsafe {
                        AudioHardwareDestroyAggregateDevice(device)
                    });
                    if stage == Stage::Done {
                        mute.device = None;
                    }
                    stage
                }
            };
            if stage == Stage::Gone {
                mute.device = None;
                mute.io = None;
                mute.running = false;
            }
            return Some(stage);
        }
        let (tap, uid) = mute.tap.clone()?;
        let stage = stage(
            || ownership(tap, kAudioTapPropertyUID, &uid),
            // SAFETY: a tap this backend made, its UID just checked.
            || unsafe { (self.destroy_tap)(tap) },
        );
        if stage != Stage::Pending {
            mute.tap = None;
        }
        Some(stage)
    }
}

fn address(selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    }
}

const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

/// Every process object Core Audio has now.
fn process_objects() -> Result<Vec<AudioObjectID>> {
    let address = address(kAudioHardwarePropertyProcessObjectList);
    let mut size = 0u32;
    // SAFETY: a live address and a place for the list's size.
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            SYSTEM,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
        )
    };
    refused("list audio processes", status)?;

    let mut objects = vec![0 as AudioObjectID; size as usize / size_of::<AudioObjectID>()];
    let mut size = (objects.len() * size_of::<AudioObjectID>()) as u32;
    if objects.is_empty() {
        return Ok(objects);
    }
    // SAFETY: the buffer holds `size` bytes of object IDs.
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(objects.as_mut_slice()).cast(),
        )
    };
    refused("list audio processes", status)?;
    objects.truncate(size as usize / size_of::<AudioObjectID>());
    Ok(objects)
}

/// The process ID of a process object, if Core Audio still knows it.
fn process_pid(object: AudioObjectID) -> Result<Option<i32>> {
    let address = address(kAudioProcessPropertyPID);
    let mut pid: i32 = 0;
    let mut size = size_of::<i32>() as u32;
    // SAFETY: the address and size describe `pid`, one process ID.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut pid).cast(),
        )
    };
    match status {
        0 if size as usize == size_of::<i32>() => Ok(Some(pid)),
        // The process ended, and its object with it.
        status if status == kAudioHardwareBadObjectError => Ok(None),
        status => Err(failed("read a process", status)),
    }
}

/// The UID Core Audio gives `object`, read with `selector`: none when the
/// object is gone or has no such UID.
///
/// # Errors
///
/// Returns that the UID could not be read.
fn uid_of(
    object: AudioObjectID,
    selector: AudioObjectPropertySelector,
) -> std::result::Result<Option<CFRetained<CFString>>, i32> {
    let address = address(selector);
    let mut uid: *const CFString = std::ptr::null();
    let mut size = size_of::<*const CFString>() as u32;
    // SAFETY: the address and size describe `uid`, one string reference
    // Core Audio returns retained.
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut uid).cast(),
        )
    };
    match status {
        // SAFETY: a string Core Audio returned with a reference this takes over.
        0 => Ok(NonNull::new(uid.cast_mut()).map(|uid| unsafe { CFRetained::from_raw(uid) })),
        status
            if status == kAudioHardwareBadObjectError
                || status == kAudioHardwareUnknownPropertyError =>
        {
            Ok(None)
        }
        status => Err(status),
    }
}

/// Whether `object` is still the one Atlas made with `uid`.
fn ownership(object: AudioObjectID, selector: AudioObjectPropertySelector, uid: &str) -> Ownership {
    match uid_of(object, selector) {
        Ok(Some(now)) if now.to_string() == uid => Ownership::Ours,
        Ok(_) => Ownership::Gone,
        Err(_) => Ownership::Unknown,
    }
}

fn failed(step: &str, status: i32) -> Error {
    Error::new(
        Code::Daemon,
        format!("Core Audio would not {step} for the helper's mute ({status})"),
    )
}

fn refused(step: &str, status: i32) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(failed(step, status))
    }
}

/// What changed in Core Audio.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// A process began or ended its audio.
    Processes,
    /// Core Audio restarted: every tap and device is gone.
    Restarted,
}

type ListenerBlock = RcBlock<dyn Fn(u32, NonNull<AudioObjectPropertyAddress>)>;

/// Core Audio's notices of changes, delivered until this is dropped.
pub struct Listener {
    queue: DispatchRetained<DispatchQueue>,
    blocks: Vec<(AudioObjectPropertySelector, ListenerBlock)>,
}

/// Tell `changed` whenever audio processes come or go, or Core Audio
/// restarts, on a queue of its own.
///
/// # Errors
///
/// Returns Core Audio's refusal to listen; nothing listens then.
pub fn listen(changed: impl Fn(Change) + Send + Sync + 'static) -> Result<Listener> {
    let changed = Arc::new(changed);
    let mut listener = Listener {
        queue: DispatchQueue::new("com.insanearts.atlas.sound", None),
        blocks: Vec::new(),
    };
    for (selector, change) in [
        (kAudioHardwarePropertyProcessObjectList, Change::Processes),
        (kAudioHardwarePropertyServiceRestarted, Change::Restarted),
    ] {
        let changed = changed.clone();
        let block: ListenerBlock =
            RcBlock::new(move |_: u32, _: NonNull<AudioObjectPropertyAddress>| changed(change));
        let address = address(selector);
        // SAFETY: a live address, queue and block; Core Audio copies the
        // block until it is removed with the same pointer.
        let status = unsafe {
            AudioObjectAddPropertyListenerBlock(
                SYSTEM,
                NonNull::from(&address),
                Some(&listener.queue),
                RcBlock::as_ptr(&block),
            )
        };
        refused("listen for changes", status)?;
        listener.blocks.push((selector, block));
    }
    Ok(listener)
}

impl Drop for Listener {
    fn drop(&mut self) {
        for (selector, block) in &self.blocks {
            let address = address(*selector);
            // SAFETY: the address, queue and block it was added with.
            unsafe {
                AudioObjectRemovePropertyListenerBlock(
                    SYSTEM,
                    NonNull::from(&address),
                    Some(&self.queue),
                    RcBlock::as_ptr(block),
                )
            };
        }
    }
}

#[cfg(test)]
mod tests;
