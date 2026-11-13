//! The steps that run as root: creating the helper account and readying it
//! for its first login under the person's one approval, and closing the
//! desktop logins macOS leaves behind when an account is deleted while
//! logged in. Only Apple's own tools run as root, from a script fixed here
//! that takes everything it acts on as arguments. Atlas itself runs only
//! as the helper, from a root-owned copy verified after copying, readying
//! that account (`setup-helper`).
//!
//! Root re-checks every user ID it is asked to close, just before each step
//! that ends a login: a person's user ID that no directory has an account
//! for, not the account on screen and not the one asking. Only a login
//! under the helper's user ID is closed while its account exists, and only
//! while that account is still the one Atlas recorded, by asking launchd to
//! end it, never by killing its processes.
use atlas_client::setup::{
    self,
    executor::{
        APPROVAL, BECOME_ROOT, EXECUTOR, MARK, ROOT_PATH, finish_executor, last_words, staging,
        start_executor, tracked_output, wait_until_ended,
    },
};
use atlas_core::app_bundle::signed_executable;
use atlas_daemon::identity::Identity;
use atlas_native::approval::{self, Refusal};
use serde_json::Value;
use std::{path::Path, process::Command, time::Instant};

fn steps() -> String {
    [staging().as_str(), STEPS].concat()
}

/// The steps, as root. Arguments, one of:
/// - `close TARGET…`: close logins, each named as one of
///   - `orphan:UID`, a deleted account's login;
///   - `leftover:UID:NAME:GENERATEDUID:PID`, a deleted account's login at
///     process PID under the user ID the helper NAME was given after it;
///   - `helper:UID:NAME:GENERATEDUID`, the helper's own login, whose Atlas
///     cannot log it out;
/// - `remove-helper UID NAME GENERATEDUID`: delete the logged-out helper;
/// - `create NAME EXECUTABLE UID FULL-NAME PICTURE [UID…]` (the picture in
///   base64) or `choose NAME EXECUTABLE UID GENERATEDUID [UID…]`, where UID
///   (and GENERATEDUID) is the account the asking Atlas chose or checked,
///   and the user IDs after it are deleted accounts' logins to close first.
///
/// Every check runs again just before each step that ends a login or
/// deletes an account, so nothing it relied on can have changed since: who
/// is on screen, whether the user ID has an account, and which account that
/// is. A look at the processes that fails stops everything; it is never
/// taken for none.
///
/// Each outcome is a line starting with [`MARK`].
const STEPS: &str = r##"refuse() { print -r -- "@atlas error $*"; exit 1 }
: ${asker:=$UID}
# Who owns the screen now, into $console.
on_screen() {
  console=$(/usr/bin/stat -f %u /dev/console </dev/null)
  [[ $console == <-> ]] || refuse "macOS could not say who is on screen"
}
# Whether a process of user ID $1 runs: one named $2, or any.
runs() {
  if (( $# > 1 )); then
    /usr/bin/pgrep -qxu $1 $2 </dev/null
  else
    /usr/bin/pgrep -qU $1 </dev/null
  fi
  case $? in
    (0) return 0 ;;
    (1) return 1 ;;
    (*) refuse "macOS could not list the processes of user ID $1" ;;
  esac
}
# No configured directory resolves this exact positive user ID, and no
# process runs as it. Search directly rather than parsing system IDs.
unused_uid() {
  local found
  found=$(/usr/bin/dscl /Search -search /Users UniqueID $1 </dev/null) || refuse "macOS could not look up user ID $1"
  [[ -z $found ]] || refuse "user ID $1 is taken now; try again"
  runs $1 && refuse "user ID $1 has running processes"
  return 0
}
# A person's user ID, and not the one asking.
person() {
  [[ $1 == <501-60000> ]] || refuse "user ID $1 is not a person's account"
  [[ $1 != $asker ]] || refuse "user ID $1 is the account asking"
}
# User ID $1 is not on screen now; $2 says why when it is. Every check
# ends with it, after its lookups and waits, so who is on screen is read
# right before the step the check allows.
off_screen() {
  on_screen
  [[ $1 != $console ]] || refuse "${2:-user ID $1 is on screen}"
}
# A deleted account's login, which root may close by force: no account in
# any directory, looked up twice so a directory that is slow to forget a
# deleted account is never trusted.
orphan() {
  local u=$1 try found
  person $u
  for try in 1 2; do
    [[ -z $(/usr/bin/id -nu -- $u </dev/null) ]] || refuse "user ID $u has an account"
    found=$(/usr/bin/dscl /Search -search /Users UniqueID $u </dev/null) || refuse "macOS could not look up user ID $u"
    [[ -z $found ]] || refuse "user ID $u has an account"
    (( try == 1 )) && /bin/sleep 0.5
  done
  off_screen $u
}
# The helper's account as Atlas recorded it, looked up: user ID $1 is still
# the standard account $2 with GeneratedUID $3, the account read last.
helper_lookups() {
  local u=$1 name=$2 guid=$3
  person $u
  [[ $name =~ '^[A-Za-z0-9_][A-Za-z0-9._-]*$' && $name != <-> ]] || refuse "$name is not an account name Atlas uses"
  [[ $guid =~ '^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$' ]] || refuse "$guid is not an account's identity"
  /usr/sbin/dseditgroup -o checkmember -m $name admin >/dev/null </dev/null
  [[ $? == 67 ]] || refuse "user ID $u is an administrator, or macOS could not tell"
  [[ $(/usr/bin/dscl . -read /Users/$name GeneratedUID </dev/null) == "GeneratedUID: $guid" ]] || refuse "user ID $u is no longer the helper's account"
  [[ $(/usr/bin/id -nu -- $u </dev/null) == $name && $(/usr/bin/id -u -- $name </dev/null) == $u ]] || refuse "user ID $u is no longer the helper's account"
}
# The helper's account as recorded ($1 to $3), and not on screen now.
helper_account() {
  helper_lookups $1 $2 $3
  off_screen $1 "the helper is on screen; switch back to your own account first"
}
# The loginwindows of user ID $1, into $windows: none, or only process $2.
only_window() {
  windows=$(/usr/bin/pgrep -xu $1 loginwindow </dev/null)
  (( $? <= 1 )) || refuse "macOS could not list the processes of user ID $1"
  [[ -z $windows || $windows == $2 ]] || refuse "user ID $1 has another login now"
}
# A login left by a deleted account under the helper's user ID $1: the
# helper's account as recorded ($2, $3), its only login the one at process
# $4 (or none left), and that login older than the account by ten seconds
# or more, so it cannot be the helper's own. Its logins are looked at again
# after the lookups, and who is on screen last.
leftover() {
  local u=$1 name=$2 pid=$4 windows clock days=0 parts ran made now
  helper_lookups $1 $2 $3
  [[ $pid == <-> ]] || refuse "$pid is not a process"
  only_window $u $pid
  if [[ -n $windows ]]; then
    clock=${$(/bin/ps -o etime= -p $pid </dev/null)//[[:space:]]/}
    if [[ $clock == *-* ]]; then days=${clock%%-*} clock=${clock#*-}; fi
    parts=("${(@s.:.)clock}")
    (( $#parts == 2 )) && parts=(0 $parts)
    [[ $days == <-> && $#parts == 3 && ${(j::)parts} == <-> ]] || refuse "macOS could not say when the login of user ID $u started"
    ran=$(( days * 86400 + parts[1] * 3600 + parts[2] * 60 + parts[3] ))
    made=$(/usr/bin/dscl . -read /Users/$name accountPolicyData </dev/null)
    [[ $made == *'<key>creationTime</key>'* ]] || refuse "macOS could not say when $name was made"
    made=${${made#*'<key>creationTime</key>'}#*'<real>'}
    made=${${made%%'</real>'*}//[[:space:]]/}
    made=${made%%.*}
    now=$(/bin/date +%s </dev/null)
    [[ $made == <-> && $now == <-> ]] || refuse "macOS could not say when $name was made"
    (( now - ran + 10 <= made )) || refuse "the login of user ID $u is the helper's own"
  fi
  helper_lookups $1 $2 $3
  only_window $u $pid
  off_screen $u "the helper is on screen; switch back to your own account first"
}
# Whether no loginwindow and no Dock of user ID $1 run, in one look.
out_now() {
  runs $1 loginwindow && return 1
  runs $1 Dock && return 1
  return 0
}
# Whether no loginwindow and no Dock of user ID $1 run, seen twice half a
# second apart: a Dock alone can restart in a login that lives on.
gone() {
  local try
  for try in 1 2; do
    runs $1 loginwindow && return 1
    runs $1 Dock && return 1
    (( try == 1 )) && /bin/sleep 0.5
  done
  return 0
}
# Wait up to $2 half seconds for user ID $1's login to go.
wait_gone() {
  local i
  for i in {1..$2}; do gone $1 && return 0; /bin/sleep 0.5; done
  gone $1
}
# Wait up to two seconds for every process of user ID $1 to end.
wait_empty() {
  local i
  for i in {1..4}; do runs $1 || return 0; /bin/sleep 0.5; done
  ! runs $1
}
# Close a deleted account's login: launchd ends it first; what is left is
# stopped, then killed, each step only once the account is checked gone
# again.
close_orphan() {
  local u=$1
  orphan $u
  /bin/launchctl bootout user/$u </dev/null >/dev/null
  wait_gone $u 20
  if runs $u; then
    orphan $u
    /usr/bin/pkill -TERM -U $u </dev/null
    if ! wait_empty $u; then
      orphan $u
      /usr/bin/pkill -KILL -U $u </dev/null
      wait_empty $u || refuse "macOS didn't close the login of user ID $u"
    fi
  fi
  print -r -- "@atlas closed $u"
}
# End the login of user ID $1 through launchd, never by killing: its GUI
# session, then the whole user. The check after it ($2 onward) runs again
# before each.
end_login() {
  local u=$1
  shift
  gone $u && return 0
  "$@"
  /bin/launchctl bootout gui/$u </dev/null >/dev/null
  wait_gone $u 30 && return 0
  "$@"
  /bin/launchctl bootout user/$u </dev/null >/dev/null
  wait_gone $u 30 || refuse "macOS didn't end the login of user ID $u"
}
# Check one login to close, as `close` names it.
check_target() {
  local parts=("${(@s.:.)1}")
  case $parts[1]:$#parts in
    (orphan:2) orphan $parts[2] ;;
    (leftover:5) leftover "${(@)parts[2,5]}" ;;
    (helper:4) helper_account "${(@)parts[2,4]}" ;;
    (*) refuse "$1 is not a login Atlas closes" ;;
  esac
}
# Close one login, as `close` names it.
close_target() {
  local parts=("${(@s.:.)1}")
  case $parts[1] in
    (orphan) close_orphan $parts[2] ;;
    (leftover) end_login $parts[2] leftover "${(@)parts[2,5]}"; print -r -- "@atlas closed $parts[2]" ;;
    (helper) end_login $parts[2] helper_account "${(@)parts[2,4]}"; print -r -- "@atlas closed $parts[2]" ;;
  esac
}
mode=$1
case $mode in
  (close)
    shift
    (( $# )) || refuse "no login to close"
    for target in "$@"; do check_target $target; done
    print -r -- "@atlas acting"
    for target in "$@"; do close_target $target; done
    print -r -- "@atlas done"
    exit 0
    ;;
  (remove-helper)
    u=$2 name=$3 guid=$4
    helper_account $u $name $guid
    gone $u || refuse "the helper is still logged in"
    # Last, right before the deletion: the account, then its processes,
    # then who is on screen, each read after the one before.
    helper_lookups $u $name $guid
    out_now $u || refuse "the helper is still logged in"
    off_screen $u "the helper is on screen; switch back to your own account first"
    /usr/sbin/sysadminctl -deleteUser $name </dev/null >/dev/null
    [[ -z $(/usr/bin/id -u -- $name </dev/null) ]] || refuse "macOS did not delete $name"
    if runs $u; then close_orphan $u; fi
    print -r -- "@atlas removed $u"
    exit 0
    ;;
esac
name=$2 executable=$3 expected=$4
# Checked again here, as root: an account name (never only digits, which id
# reads as a user ID), and only a person's account: macOS gives the user IDs
# below 501 to itself.
[[ $name =~ '^[A-Za-z0-9_][A-Za-z0-9._-]*$' && $name != <-> ]] || refuse "$name is not an account name Atlas uses"
[[ $expected == <501-60000> ]] || refuse "$name is not a person's account"
if [[ $mode == create || $mode == create-terminal ]]; then orphans=("${@[7,-1]}"); else guid=$5 orphans=("${@[6,-1]}"); fi
# Verify a protected copy before closing logins, creating an account or
# executing the worker. Terminal creation runs only Apple's account tool.
if [[ $mode != create-terminal ]]; then stage_executable; fi
# Deleted accounts' logins close before anything is made: a new account's
# home is never made while one of them runs.
for u in $orphans; do orphan $u; done
for u in $orphans; do close_orphan $u; done
# Every orphan must be gone before an account or its home can be made.
for u in $orphans; do
  orphan $u
  gone $u || refuse "a deleted account is still logged in"
  runs $u && refuse "a deleted account still has running processes"
done
if [[ $mode == create-terminal ]]; then
  [[ -z $(/usr/bin/id -u -- $name </dev/null) ]] || refuse "$name already exists"
  arguments=(-addUser $name -fullName "$5" -UID $expected -password -)
  [[ -n $6 ]] && arguments+=(-picture "$6")
  unused_uid $expected
  /usr/sbin/sysadminctl $arguments || refuse "macOS did not create $name"
  [[ $(/usr/bin/id -u -- $name </dev/null) == $expected ]] || refuse "macOS did not create the chosen account"
  exit 0
fi
uid=$(/usr/bin/id -u -- $name)
if [[ $mode == create ]]; then
  [[ -z $uid ]] || refuse "$name already exists"
  full=$5 picture=$6
  [[ -n $full && $full != -* ]] || refuse "the full name must not be empty or start with -"
  folder=$(/usr/bin/mktemp -d)
  [[ -n $folder ]] || refuse "macOS could not make a temporary folder"
  arguments=(-addUser $name -fullName $full -UID $expected -password -)
  print -rn -- $picture | /usr/bin/base64 -d -o $folder/picture
  [[ -s $folder/picture ]] && arguments+=(-picture $folder/picture)
  # script gives sysadminctl a terminal. script refuses a socket (or a named
  # pipe) as its input, so it runs as a coprocess on plain pipes: its output
  # passes on, and each password question gets the line Atlas sends then.
  # sysadminctl's status follows its last words on a line of its own.
  # A sysadminctl silent for five minutes is stopped rather than left waiting.
  unused_uid $expected
  coproc /usr/bin/script -q /dev/null /bin/zsh -f -c '"$@"; made=$?; print; print -r -- "@atlas created $made"' zsh /usr/sbin/sysadminctl $arguments
  child=$!
  heard="" answers=0
  while IFS= read -r -t 300 -k 1 -p c 2>/dev/null; do
    print -rn -- $c
    heard="${heard[-8,-1]}$c"
    if [[ ${heard:l} == *password: ]] && (( answers < 3 )); then
      IFS= read -r line || break
      print -rp -- $line
      line="" heard=""
      (( answers++ ))
    fi
  done
  print
  kill $child 2>/dev/null
  /bin/rm -rf -- $folder
  uid=$(/usr/bin/id -u -- $name)
  [[ -n $uid ]] || refuse "macOS did not create $name"
fi
[[ $uid == $expected ]] || refuse "$name does not have the user ID Atlas checked"
# The account just made is known by the GeneratedUID macOS gave it; one
# chosen, by the one Atlas checked.
if [[ $mode == create ]]; then
  guid=$(/usr/bin/dscl . -read /Users/$name GeneratedUID </dev/null)
  guid=${guid#GeneratedUID: }
fi
[[ $guid =~ '^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$' ]] || refuse "macOS could not say which account $name is"
# The account Atlas chose or made, checked again right before each step on
# it: a standard account (dseditgroup answers 67 for one outside the
# group), and, read last, still $name with user ID $expected and
# GeneratedUID $guid.
chosen() {
  /usr/sbin/dseditgroup -o checkmember -m $name admin >/dev/null </dev/null
  [[ $? == 67 ]] || refuse "$name is an administrator, or macOS could not tell"
  [[ $(/usr/bin/dscl . -read /Users/$name GeneratedUID </dev/null) == "GeneratedUID: $guid" ]] || refuse "$name is not the account Atlas checked"
  [[ $(/usr/bin/id -u -- $name </dev/null) == $expected && $(/usr/bin/id -nu -- $expected </dev/null) == $name ]] || refuse "$name does not have the user ID Atlas checked"
}
chosen
/usr/sbin/createhomedir -c -u $name >/dev/null </dev/null
request="{\"action\":\"first-login\",\"uid\":$uid,\"opener\":true}"
# Its answer is on standard output, a refusal on standard error: kept apart,
# so a stray warning never spoils the answer.
errors=$(/usr/bin/mktemp)
# Through the account's login when it has one, so launchctl reaches it
# there. The readying checks itself, as the account, that it is still the
# one with that GeneratedUID.
identity="{\"uid\":$uid,\"name\":\"$name\",\"generatedUid\":\"$guid\"}"
if runs $uid loginwindow; then
  chosen
  readied=$(/bin/launchctl asuser $uid /usr/bin/sudo -n -u $name $executable setup-helper $request --account $identity 2>$errors </dev/null)
else
  chosen
  readied=$(/usr/bin/sudo -n -u $name $executable setup-helper $request --account $identity --logged-out 2>$errors </dev/null)
fi
[[ -n $readied ]] || readied=$(<$errors)
/bin/rm -f -- $errors
print -r -- "@atlas readied $readied"
"##;

/// The helper account's picture, Atlas's mark, carried in Atlas's signed
/// executable rather than read from the app's folder.
const PICTURE: &[u8] = include_bytes!("../../../../../app/Resources/HelperPicture.png");

/// How the steps reach root: through macOS's one approval, its dialog
/// saying what will happen, or through sudo in a terminal, asking for the
/// person's password there only when `ask` allows it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Via<'a> {
    Approval(&'a str),
    Sudo { ask: bool },
}

/// Create the account `name` with user ID `uid`, called `full_name`, typing
/// `password` into macOS's question, and ready it for its first login, once
/// the deleted accounts' logins `orphans` are closed.
pub(super) fn create(
    name: &str,
    uid: u32,
    full_name: &str,
    password: &[u8],
    orphans: &[u32],
) -> Result<(), Unclosed> {
    let picture = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(PICTURE)
    };
    let executable = executable().map_err(untouched)?;
    let uid = uid.to_string();
    let prompt = format!("Atlas will create “{name}” and get it ready to be its helper.");

    readying(
        &prompt,
        &["create", name, &executable, &uid, full_name, &picture],
        orphans,
        Some(password),
    )
}

/// Close orphan logins and create an account in one approved terminal run.
/// The caller holds each orphan's controller and durable claim throughout.
/// sysadminctl asks for the new password directly, without putting it in argv.
pub(super) fn create_in_terminal(
    name: &str,
    uid: u32,
    full_name: &str,
    orphans: &[u32],
) -> Result<(), Unclosed> {
    let script = [
        EXECUTOR,
        "exec </dev/tty || exit 1\n",
        BECOME_ROOT.replace("exec 2>/dev/null\n", "").as_str(),
        &steps(),
    ]
    .concat();
    let asker = format!("SUDO_UID={}", nix::unistd::getuid().as_raw());
    let uid = uid.to_string();
    let picture = atlas_client::user::setup::account::helper_picture().unwrap_or_default();
    let orphans = numbers(orphans);
    let mut arguments = vec![
        "/usr/bin/env",
        "-i",
        ROOT_PATH,
        &asker,
        "/bin/zsh",
        "-p",
        "-f",
        "-c",
        &script,
        "zsh",
        "create-terminal",
        name,
        "",
        &uid,
        full_name,
        &picture,
    ];
    arguments.extend(orphans.iter().map(String::as_str));
    let (child, pid) = start_executor(
        Command::new("/usr/bin/sudo")
            .args(&arguments)
            .stderr(std::process::Stdio::piped()),
    )
    .map_err(|refusal| Unclosed {
        refusal,
        exit: Exit::Unknown,
    })?;
    let outcome =
        setup::executor::quietly_attended(child, "The helper account could not be created");
    wait_until_ended(pid);
    outcome.map_err(|error| Unclosed {
        refusal: Refusal::Failed(error.error),
        exit: Exit::AfterEffects,
    })
}

/// Ready the existing account `helper`, as checked, for its next login,
/// once the deleted accounts' logins `orphans` are closed.
pub(super) fn ready(helper: &Identity, orphans: &[u32]) -> Result<(), Unclosed> {
    let executable = executable().map_err(untouched)?;
    let name = helper.name.as_str();
    let uid = helper.uid.to_string();
    let prompt = format!("Atlas will get “{name}” ready to be its helper.");

    readying(
        &prompt,
        &["choose", name, &executable, &uid, &helper.generated_uid],
        orphans,
        None,
    )
}

/// One approved run that readies an account, closing `orphans` first.
fn readying(
    prompt: &str,
    step: &[&str],
    orphans: &[u32],
    secret: Option<&[u8]>,
) -> Result<(), Unclosed> {
    let orphans = numbers(orphans);
    let mut arguments = step.to_vec();
    arguments.extend(orphans.iter().map(String::as_str));

    readied(run_to_end(Via::Approval(prompt), &arguments, secret))
}

/// A refusal before anything ran as root.
fn untouched(refusal: Refusal) -> Unclosed {
    Unclosed {
        refusal,
        exit: Exit::WithoutEffects,
    }
}

/// What a run (`ran`, and whether its process was seen to exit) said. A
/// cancelled or unavailable run did nothing; one that failed, or that Atlas
/// stopped waiting for, may have acted before it ended.
fn reported((ran, exited): (Result<String, Refusal>, bool)) -> Result<(String, bool), Unclosed> {
    let text = ran.map_err(|refusal| {
        let acted = match &refusal {
            Refusal::Failed(_) => true,
            Refusal::Cancelled | Refusal::Unavailable => false,
        };
        Unclosed {
            refusal,
            exit: Exit::of(acted, exited),
        }
    })?;
    Ok((text, exited))
}

/// What a `create` or `choose` run came to, as [`outcome`] reads it. Once
/// started it may have closed logins or made the account; only its process
/// seen to exit ends it, never its output alone.
fn readied(run: (Result<String, Refusal>, bool)) -> Result<(), Unclosed> {
    let (text, ended) = reported(run)?;
    if !ended {
        return Err(Unclosed {
            refusal: Refusal::Failed("The approved executor has not exited".into()),
            exit: Exit::Unknown,
        });
    }

    outcome(&text).map_err(|refusal| Unclosed {
        refusal,
        exit: Exit::AfterEffects,
    })
}

/// One login root is asked to close, with what root checks it by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Target {
    /// A deleted account's login: no directory may have an account for its
    /// user ID. What launchd leaves of it is killed.
    Orphan(u32),
    /// A deleted account's login at process `pid`, under the user ID the
    /// helper `helper` was given after it: ended through launchd only,
    /// while the account there is still `helper` and the login is older
    /// than it.
    Leftover { helper: Identity, pid: u32 },
    /// The helper's own login, whose Atlas cannot log it out: ended through
    /// launchd only, while the account there is still this one.
    Helper(Identity),
}

impl Target {
    pub(super) fn uid(&self) -> u32 {
        match self {
            Self::Orphan(uid) => *uid,
            Self::Leftover { helper, .. } | Self::Helper(helper) => helper.uid,
        }
    }

    fn argument(&self) -> String {
        match self {
            Self::Orphan(uid) => format!("orphan:{uid}"),
            Self::Leftover { helper, pid } => format!(
                "leftover:{}:{}:{}:{pid}",
                helper.uid, helper.name, helper.generated_uid
            ),
            Self::Helper(helper) => format!(
                "helper:{}:{}:{}",
                helper.uid, helper.name, helper.generated_uid
            ),
        }
    }
}

/// How a run as root is known to have ended. Only observing its executor
/// exit proves it cannot act again: a reply that was lost, or that stopped
/// short, leaves a run that may still act.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Exit {
    /// It was not seen to exit.
    Unknown,
    /// It exited without acting on anything.
    WithoutEffects,
    /// It exited, and may have acted first (unknown counts as acted).
    AfterEffects,
}

impl Exit {
    fn of(acted: bool, ended: bool) -> Self {
        match (ended, acted) {
            (false, _) => Self::Unknown,
            (true, false) => Self::WithoutEffects,
            (true, true) => Self::AfterEffects,
        }
    }
}

/// Why logins were not closed, and how the run that was to close them ended.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Unclosed {
    pub(super) refusal: Refusal,
    pub(super) exit: Exit,
}

/// Close the logins `targets`, all in one run as root, so macOS asks the
/// person once. Root checks each again itself by its own kind's rules. The
/// user IDs it closed: every target's, or why not.
pub(crate) fn close(targets: &[Target], via: Via) -> Result<Vec<u32>, Unclosed> {
    let named: Vec<String> = targets.iter().map(Target::argument).collect();
    let mut arguments = vec!["close"];
    arguments.extend(named.iter().map(String::as_str));

    let (text, exited) = reported(run_to_end(via, &arguments, None))?;
    closed_all(&text, targets, exited)
}

/// What a `close` run said of `targets`: every one closed, or why not.
/// It ended only when its exact process was seen to exit.
fn closed_all(text: &str, targets: &[Target], ended: bool) -> Result<Vec<u32>, Unclosed> {
    let acted = text.contains("@atlas acting");
    let unclosed = |refusal| Unclosed {
        refusal,
        exit: Exit::of(acted, ended),
    };
    let uids = closed(text).map_err(unclosed)?;
    if !ended || targets.iter().any(|target| !uids.contains(&target.uid())) {
        return Err(unclosed(Refusal::Failed("The step did not finish".into())));
    }

    Ok(uids)
}

/// Delete the logged-out helper account `helper`, while its user ID is
/// still that account, and close anything of it macOS leaves running.
pub(crate) fn remove_helper(helper: &Identity, via: Via) -> Result<(), Refusal> {
    let uid = helper.uid.to_string();
    let text = run_to_end(
        via,
        &["remove-helper", &uid, &helper.name, &helper.generated_uid],
        None,
    )
    .0?;
    refused(&text)?;

    text.contains(&format!("{MARK}removed {uid}"))
        .then_some(())
        .ok_or_else(|| Refusal::Failed("The step did not finish".into()))
}

fn executable() -> Result<String, Refusal> {
    let executable = signed_executable().map_err(|error| Refusal::Failed(error.error))?;
    Ok(executable.to_string_lossy().into_owned())
}

fn numbers(uids: &[u32]) -> Vec<String> {
    uids.iter().map(u32::to_string).collect()
}

/// Run the steps and report whether they are known to have ended: they never
/// started, or their process was seen to exit. Atlas waits for that exit
/// even after it gave up on their reply, so a run cut short is never left
/// acting behind a claim Atlas settled or control it let go.
fn run_to_end(
    via: Via,
    arguments: &[&str],
    secret: Option<&[u8]>,
) -> (Result<String, Refusal>, bool) {
    let script = [EXECUTOR, BECOME_ROOT, &steps()].concat();

    match via {
        Via::Approval(prompt) => {
            let mut argv = vec![
                "-i", ROOT_PATH, "/bin/zsh", "-p", "-f", "-c", &script, "zsh",
            ];
            argv.extend_from_slice(arguments);

            let stream = match approval::run(Path::new("/usr/bin/env"), &argv, prompt) {
                Ok(stream) => stream,
                Err(refusal) => return (Err(refusal), true),
            };
            finish_executor(stream, secret, Instant::now() + APPROVAL)
        }
        Via::Sudo { ask } => {
            let asker = format!("SUDO_UID={}", nix::unistd::getuid().as_raw());
            let mut command = Command::new("/usr/bin/sudo");
            if !ask {
                command.arg("-n");
            }

            // Waits for sudo, and the steps under it, to exit.
            command
                .args(["/usr/bin/env", "-i", ROOT_PATH, &asker])
                .args(["/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
                .args(arguments)
                .stderr(std::process::Stdio::piped());
            let output = match tracked_output(&mut command) {
                Ok(output) => output,
                Err(refusal) => return (Err(refusal), false),
            };
            let text = String::from_utf8_lossy(&output.stdout).into_owned();

            if !text.contains(MARK) {
                return (Err(Refusal::Unavailable), true);
            }

            (Ok(text), true)
        }
    }
}

fn refused(text: &str) -> Result<(), Refusal> {
    match marked(text, "@atlas error ") {
        Some(why) => Err(Refusal::Failed(why.to_owned())),
        None => Ok(()),
    }
}

/// A marker's first line, including when sysadminctl did not end its prompt.
fn marked<'a>(text: &'a str, mark: &str) -> Option<&'a str> {
    text.split_once(mark)
        .map(|(_, rest)| rest.lines().next().unwrap_or_default().trim())
}

/// The user IDs the steps' lines say were closed, or their refusal. Lines
/// that say nothing were closed are a step that never finished.
fn closed(text: &str) -> Result<Vec<u32>, Refusal> {
    refused(text)?;

    let uids: Vec<u32> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("@atlas closed "))
        .filter_map(|uid| uid.trim().parse().ok())
        .collect();
    if uids.is_empty() {
        return Err(Refusal::Failed("The step did not finish".into()));
    }

    Ok(uids)
}

/// What the steps' lines say: a refusal (when a creation made no account,
/// sysadminctl's own last words say why), or the readying's reply, which
/// must be ok. The reply is `setup-helper`'s JSON, the rest of the text
/// after its mark.
fn outcome(text: &str) -> Result<(), Refusal> {
    let before = text.split(MARK).next().unwrap_or_default();
    if let Some(refused) = marked(text, "@atlas error ") {
        let why = match marked(text, "@atlas created ") {
            Some(status) => match last_words(before) {
                Some(said) => said.to_owned(),
                None if status != "0" => {
                    format!("{refused} (sysadminctl stopped with status {status})")
                }
                None => refused.to_owned(),
            },
            None => refused.to_owned(),
        };
        return Err(Refusal::Failed(why));
    }

    let readied = text
        .split_once("@atlas readied ")
        .map(|(_, reply)| reply.trim())
        .ok_or_else(|| Refusal::Failed("The approved step did not finish".into()))?;
    let reply: Value = serde_json::from_str(readied).unwrap_or(Value::Null);
    if reply["ok"] != true {
        let why = match reply["error"].as_str() {
            Some(error) => error,
            None => last_words(readied).unwrap_or("Atlas could not get the account ready"),
        };
        return Err(Refusal::Failed(why.to_owned()));
    }

    Ok(())
}

#[cfg(test)]
#[path = "as_root_tests.rs"]
mod tests;
