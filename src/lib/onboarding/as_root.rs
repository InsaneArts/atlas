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
use super::{APPROVAL, MAX_ANSWERS, answer, last_words, signed_executable, without};
use crate::{
    approval::{self, Refusal},
    init,
    setup::identity::Identity,
};
use serde_json::Value;
use std::{os::unix::net::UnixStream, path::Path, process::Command, time::Instant};

/// What each line the script reports a step's outcome with starts with.
const MARK: &str = "@atlas ";

/// Identify the approved executor before it can act. If its caller cannot
/// read the identity or goes away before acknowledging it, the script exits
/// without reaching any privileged work.
pub(crate) const EXECUTOR: &str = r##"print -r -- "@atlas executor $$"
IFS= read -r acknowledgement || exit 1
[[ $acknowledgement == "@atlas continue $$" ]] || exit 1
"##;

/// Root for good, with root's own environment: zsh sets every user ID on
/// `UID=0` when it may, and a refusal would end zsh itself, so it is tried
/// in a subshell first. The account asking is noted first: sudo names it,
/// and macOS's approval keeps it as the real user ID. Standard error goes
/// nowhere (it is the asking Atlas's own), and nothing of the asking process
/// carries over that could shape what root makes: its file mode mask or its
/// limits.
pub(crate) const BECOME_ROOT: &str = r##"asker=${SUDO_UID:-$UID}
if ! ( UID=0 ) 2>/dev/null; then print -r -- "@atlas error macOS did not let Atlas act as an administrator"; exit 1; fi
UID=0
GID=0
exec 2>/dev/null
umask 022
ulimit -f unlimited
export HOME=/var/root LOGNAME=root USER=root
unset OLDPWD
cd /
"##;

/// Protect and verify the root run's worker, then remove its copy on exit.
/// The directory is private until copying, ownership, permissions and
/// signature verification finish; neither account can replace or edit it.
const STAGING: &str = r##"staged=
trap '[[ -z $staged ]] || /bin/rm -rf -- "$staged"' EXIT
trap 'exit 1' HUP INT TERM
trap '' PIPE
stage_executable() {
  [[ $executable == /*/Contents/MacOS/atlas ]] || refuse "Atlas has no app bundle"
  local source=${executable%/Contents/MacOS/atlas} bad
  staged=$(/usr/bin/mktemp -d /private/var/tmp/atlas-approved.XXXXXXXX) || refuse "macOS could not protect a copy of Atlas"
  # Remove any inherited ACL before copying anything into this directory.
  /bin/chmod -N $staged && /bin/chmod 700 $staged || refuse "macOS could not protect the copy's folder"
  /usr/bin/ditto --noacl --noextattr --norsrc $source $staged/Atlas.app || refuse "Atlas could not be copied"
  # Links or special files could escape the protected bundle. Inspect them
  # before any recursive ownership or permission change.
  bad=$(/usr/bin/find $staged ! -type d ! -type f -print) || refuse "Atlas's copy could not be inspected"
  [[ -z $bad ]] || refuse "Atlas's copy contains links or special files"
  /usr/sbin/chown -R 0:0 $staged && /bin/chmod -RN $staged && /bin/chmod -R u+rwX,go+rX,go-w,u-s,g-s $staged/Atlas.app || refuse "Atlas's copy could not be protected"
  /usr/bin/codesign --verify --strict --deep -R '=ATLAS_WORKER_REQUIREMENT' $staged/Atlas.app &&
    /usr/bin/codesign --verify --strict -R '=ATLAS_APP_REQUIREMENT' $staged/Atlas.app/Contents/MacOS/AtlasApp || refuse "Atlas's protected copy is not signed by Atlas's developer"
  /bin/chmod 755 $staged || refuse "Atlas's verified copy could not be opened"
  executable=$staged/Atlas.app/Contents/MacOS/atlas
}
"##;

pub(crate) fn staging() -> String {
    STAGING
        .replace(
            "ATLAS_WORKER_REQUIREMENT",
            &crate::setup::requirement_for(crate::setup::IDENTIFIER),
        )
        .replace(
            "ATLAS_APP_REQUIREMENT",
            &crate::setup::requirement_for(crate::setup::APP_IDENTIFIER),
        )
}

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
const PICTURE: &[u8] = include_bytes!("../../../app/Resources/HelperPicture.png");

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
    let orphans = numbers(orphans);
    let mut arguments = vec!["create", name, &executable, &uid, full_name, &picture];
    arguments.extend(orphans.iter().map(String::as_str));
    let prompt = format!("Atlas will create “{name}” and get it ready to be its helper.");

    readied(run_to_end(
        Via::Approval(&prompt),
        &arguments,
        Some(password),
    ))
}

/// Close orphan logins and create an account in one approved terminal run.
/// The caller holds each orphan's controller and durable claim throughout.
/// sysadminctl asks for the new password directly, without putting it in argv.
pub(crate) fn create_in_terminal(
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
    let picture = init::helper_picture().unwrap_or_default();
    let orphans = numbers(orphans);
    let mut arguments = vec![
        "/usr/bin/env",
        "-i",
        "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
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
        acted: true,
        ended: false,
    })?;
    let outcome = init::quietly_attended(child, "The helper account could not be created");
    wait_until_ended(pid);
    outcome.map_err(|error| Unclosed {
        refusal: Refusal::Failed(error.error),
        acted: true,
        ended: true,
    })
}

/// Ready the existing account `helper`, as checked, for its next login,
/// once the deleted accounts' logins `orphans` are closed.
pub(super) fn ready(helper: &Identity, orphans: &[u32]) -> Result<(), Unclosed> {
    let executable = executable().map_err(untouched)?;
    let name = helper.name.as_str();
    let uid = helper.uid.to_string();
    let orphans = numbers(orphans);
    let mut arguments = vec!["choose", name, &executable, &uid, &helper.generated_uid];
    arguments.extend(orphans.iter().map(String::as_str));
    let prompt = format!("Atlas will get “{name}” ready to be its helper.");

    readied(run_to_end(Via::Approval(&prompt), &arguments, None))
}

/// A refusal before anything ran as root.
fn untouched(refusal: Refusal) -> Unclosed {
    Unclosed {
        refusal,
        acted: false,
        ended: true,
    }
}

/// What a `create` or `choose` run (`ran`, and whether its process was seen
/// to exit) came to, as [`outcome`] reads it. Once started it may have
/// closed logins or made the account; it ended when its process exited or
/// its process was seen to exit; output alone cannot establish termination.
fn readied((ran, exited): (Result<String, Refusal>, bool)) -> Result<(), Unclosed> {
    let text = ran.map_err(|refusal| {
        let started = matches!(refusal, Refusal::Failed(_));
        Unclosed {
            refusal,
            acted: started,
            ended: exited,
        }
    })?;
    let ended = exited;
    if !ended {
        return Err(Unclosed {
            refusal: Refusal::Failed("The approved executor has not exited".into()),
            acted: true,
            ended: false,
        });
    }

    outcome(&text).map_err(|refusal| Unclosed {
        refusal,
        acted: true,
        ended,
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
    /// The user ID whose login this is.
    pub(crate) fn uid(&self) -> u32 {
        match self {
            Self::Orphan(uid) => *uid,
            Self::Leftover { helper, .. } | Self::Helper(helper) => helper.uid,
        }
    }

    /// The target as the steps name it.
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

/// Why logins were not closed, whether root may have acted on any of them
/// before it stopped (unknown counts as acted), and whether it is known to
/// have ended. Only observing its executor exit proves it cannot act again:
/// a reply that was lost, or that stopped short,
/// leaves a run that may still act.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Unclosed {
    pub(crate) refusal: Refusal,
    pub(crate) acted: bool,
    pub(crate) ended: bool,
}

/// Close the logins `targets`, all in one run as root, so macOS asks the
/// person once. Root checks each again itself by its own kind's rules. The
/// user IDs it closed: every target's, or why not.
pub(crate) fn close(targets: &[Target], via: Via) -> Result<Vec<u32>, Unclosed> {
    let named: Vec<String> = targets.iter().map(Target::argument).collect();
    let mut arguments = vec!["close"];
    arguments.extend(named.iter().map(String::as_str));

    let (text, exited) = run_to_end(via, &arguments, None);
    let text = text.map_err(|refusal| {
        // Cancelled or unavailable: nothing ran. One Atlas stopped
        // waiting for may have acted before it ended.
        let started = matches!(refusal, Refusal::Failed(_));
        Unclosed {
            refusal,
            acted: started,
            ended: exited,
        }
    })?;
    closed_all(&text, targets, exited)
}

/// What a `close` run said of `targets`: every one closed, or why not.
/// It ended only when its exact process was seen to exit (`exited`).
fn closed_all(text: &str, targets: &[Target], exited: bool) -> Result<Vec<u32>, Unclosed> {
    let acted = text.contains("@atlas acting");
    let ended = exited;
    let unclosed = |refusal| Unclosed {
        refusal,
        acted,
        ended,
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
    let text = run(
        via,
        &["remove-helper", &uid, &helper.name, &helper.generated_uid],
        None,
    )?;
    refused(&text)?;

    text.contains(&format!("{MARK}removed {uid}"))
        .then_some(())
        .ok_or_else(|| Refusal::Failed("The step did not finish".into()))
}

/// This Atlas.app's executable, signed by its developer, as text.
fn executable() -> Result<String, Refusal> {
    let executable = signed_executable().map_err(|error| Refusal::Failed(error.error))?;
    Ok(executable.to_string_lossy().into_owned())
}

/// User IDs as arguments.
fn numbers(uids: &[u32]) -> Vec<String> {
    uids.iter().map(u32::to_string).collect()
}

/// Run the steps with `arguments` as root, reached `via` the approval or
/// sudo, answering any password question with `secret` (the approval's
/// channel only). What the steps said.
fn run(via: Via, arguments: &[&str], secret: Option<&[u8]>) -> Result<String, Refusal> {
    run_to_end(via, arguments, secret).0
}

/// [`run`], and whether the steps are known to have ended: they never
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
            let approved_script = [EXECUTOR, BECOME_ROOT, &steps()].concat();
            let mut argv = vec![
                "-i",
                "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
                "/bin/zsh",
                "-p",
                "-f",
                "-c",
                &approved_script,
                "zsh",
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
                .args([
                    "/usr/bin/env",
                    "-i",
                    "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
                    &asker,
                ])
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

/// Run a privileged command with the durable executor handshake. Its script
/// starts with [`EXECUTOR`]. Interactive tools use the controlling tty; stdin
/// carries only the acknowledgement sent after all bound claims are synced.
pub(crate) fn tracked_output(command: &mut Command) -> Result<std::process::Output, Refusal> {
    let (child, pid) = start_executor(command)?;
    let output = child
        .wait_with_output()
        .map_err(|error| Refusal::Failed(error.to_string()))?;
    wait_until_ended(pid);
    Ok(output)
}

/// Start the fixed executor and durably identify it before permitting work.
#[allow(clippy::expect_used, reason = "spawn creates both requested pipes")]
pub(super) fn start_executor(
    command: &mut Command,
) -> Result<(std::process::Child, nix::unistd::Pid), Refusal> {
    use std::{io::Write, process::Stdio};
    let failed = |error: std::io::Error| Refusal::Failed(error.to_string());
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(failed)?;
    let mut output = child.stdout.take().expect("piped executor output");
    let mut input = child.stdin.take().expect("piped executor input");
    let started = executor(&mut output).and_then(|pid| {
        crate::control::record_executor(pid).map_err(|error| Refusal::Failed(error.to_string()))?;
        writeln!(input, "@atlas continue {}", pid.as_raw()).map_err(failed)?;
        Ok(pid)
    });
    child.stdin = Some(input);
    child.stdout = Some(output);
    match started {
        Ok(pid) => Ok((child, pid)),
        Err(refusal) => {
            let _ = child.wait_with_output();
            Err(refusal)
        }
    }
}

/// A sudo dispatcher can outlive its launcher: wait for the executor itself,
/// treating inaccessible process state as live rather than as proof of exit.
pub(super) fn wait_until_ended(pid: nix::unistd::Pid) {
    while nix::sys::signal::kill(pid, None) != Err(nix::errno::Errno::ESRCH) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Keep the caller, its ownership and its claims alive until this exact
/// executor exits, even if its reply times out or its output closes early.
fn finish_executor(
    mut stream: UnixStream,
    secret: Option<&[u8]>,
    deadline: Instant,
) -> (Result<String, Refusal>, bool) {
    let executor = match executor(&mut stream) {
        Ok(executor) => executor,
        Err(refusal) => return (Err(refusal), false),
    };
    if let Err(error) = crate::control::record_executor(executor) {
        drop(stream);
        return (Err(Refusal::Failed(error.to_string())), reap(executor));
    }
    use std::io::Write;
    if let Err(error) = writeln!(stream, "@atlas continue {}", executor.as_raw()) {
        return (Err(Refusal::Failed(error.to_string())), reap(executor));
    }
    let (_, text) = converse_until(stream, secret, deadline);
    (text, reap(executor))
}

/// Talk with the approved step over its channel until it ends: each
/// password question answered with `secret`, and everything kept. Whether
/// it ended (rather than Atlas giving up on it), and what it said. The step
/// never writes the secret back; only what sysadminctl wrote before the
/// first step's line could hold it, and it is cut out there.
#[cfg(test)]
fn converse(stream: UnixStream, secret: Option<&[u8]>) -> (bool, Result<String, Refusal>) {
    converse_until(stream, secret, Instant::now() + APPROVAL)
}

fn converse_until(
    mut stream: UnixStream,
    secret: Option<&[u8]>,
    deadline: Instant,
) -> (bool, Result<String, Refusal>) {
    let (secret, answers) = secret.map_or((&[][..], 0), |secret| (secret, MAX_ANSWERS));
    let channel = match stream.try_clone() {
        Ok(channel) => channel,
        Err(error) => return (false, Err(Refusal::Failed(error.to_string()))),
    };
    let receiver = init::forward(channel);
    let mut seen = Vec::new();
    let answered = answer(&mut stream, &receiver, secret, answers, deadline, &mut seen);
    drop(stream);

    let marked = seen
        .windows(MARK.len())
        .position(|window| window == MARK.as_bytes())
        .unwrap_or(seen.len());
    let mut kept = without(&seen[..marked], secret);
    kept.extend_from_slice(&seen[marked..]);
    seen.fill(0);
    let text = String::from_utf8_lossy(&kept).into_owned();
    kept.fill(0);

    let finished = answered.is_ok();
    let said = match answered {
        Ok(()) => Ok(text),
        // A step that stopped early (refusing the account, say) closed its
        // channel before the password went in: its own line says why.
        Err(_) if text.contains(MARK) => Ok(text),
        Err(_) => Err(Refusal::Failed(
            "Atlas stopped waiting for the approved step, which may still finish".into(),
        )),
    };
    (finished, said)
}

/// Reap the approved step, this process's child, waiting for it to exit
/// even when its reply never came: a step that may still act holds this
/// process, and with it the control and claims it runs under, until it
/// ends. Whether it was seen to exit.
fn reap(executor: nix::unistd::Pid) -> bool {
    use nix::{
        errno::Errno,
        sys::wait::{WaitStatus, waitpid},
    };
    loop {
        match waitpid(executor, None) {
            Ok(WaitStatus::Exited(..) | WaitStatus::Signaled(..)) => return true,
            Err(Errno::EINTR) => continue,
            _ => return false,
        }
    }
}

/// Read the executor's own PID before its script can mutate anything.
/// Waiting for this exact child cannot accidentally reap an unrelated process.
fn executor(stream: &mut impl std::io::Read) -> Result<nix::unistd::Pid, Refusal> {
    let mut line = Vec::new();
    for _ in 0..64 {
        let mut byte = [0];
        if stream.read_exact(&mut byte).is_err() {
            break;
        }
        if byte[0] == b'\n' {
            let pid = String::from_utf8_lossy(&line)
                .trim_end_matches('\r')
                .strip_prefix("@atlas executor ")
                .and_then(|pid| pid.parse::<i32>().ok())
                .filter(|pid| *pid > 0);
            if let Some(pid) = pid {
                return Ok(nix::unistd::Pid::from_raw(pid));
            }
            break;
        }
        line.push(byte[0]);
    }
    Err(Refusal::Failed(
        "Atlas could not identify the approved executor; its work remains pending".into(),
    ))
}

/// The refusal in the steps' lines, if they refused.
fn refused(text: &str) -> Result<(), Refusal> {
    match text
        .split_once("@atlas error ")
        .map(|(_, rest)| rest.lines().next().unwrap_or_default().trim())
    {
        Some(why) => Err(Refusal::Failed(why.to_owned())),
        None => Ok(()),
    }
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
    // What follows a mark, to the end of its line, wherever the mark is.
    let after = |mark: &str| {
        text.split_once(mark)
            .map(|(_, rest)| rest.lines().next().unwrap_or_default().trim())
    };

    if let Some(refused) = after("@atlas error ") {
        let why = match after("@atlas created ") {
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
mod tests {
    #[test]
    fn combined_setup_with_leftover_logins_requests_authorization_once() {
        for arguments in [
            vec![
                "create",
                "atlas",
                "/unused/Atlas",
                "7004",
                "Atlas",
                "picture",
                "7001",
                "7002",
            ],
            vec![
                "choose",
                "atlas",
                "/unused/Atlas",
                "7004",
                "identity",
                "7001",
                "7002",
            ],
        ] {
            // Intercept at the Authorization boundary: no dialog or privileged program can run.
            let ((result, ended), calls) = crate::approval::without_authorization(|| {
                run_to_end(
                    Via::Approval("Prepare the helper"),
                    &arguments,
                    Some(b"secret"),
                )
            });
            assert_eq!(calls, 1);
            assert_eq!(result, Err(Refusal::Unavailable));
            assert!(ended);
        }
    }

    #[test]
    fn account_replacement_during_the_last_age_lookup_prevents_bootout() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = helper_tools(501, 7004, "opie");
        let body = tools.iter().find(|(tool, _)| *tool == "/usr/bin/dscl").unwrap().1.replace(
            "(accountPolicyData) print",
            "(accountPolicyData) n=0; [[ -e policies ]] && n=$(cat policies); (( n++ )); print $n > policies; (( n == 2 )) && touch reused; print",
        );
        replace(&mut tools, "/usr/bin/dscl", body);
        let target = Target::Leftover {
            helper: identity(7004, "opie"),
            pid: 4242,
        }
        .argument();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert!(
            text.contains("@atlas acting"),
            "replacement occurs after batch preflight: {text}"
        );
        assert!(text.contains("no longer the helper's account"), "{text}");
        assert!(!asked(dir.path(), "launchctl"), "{text}");
    }

    #[test]
    fn a_callers_death_does_not_release_a_paused_executors_claims() {
        if !crate::test_support::isolated(
            "onboarding::as_root::tests::a_callers_death_does_not_release_a_paused_executors_claims",
        ) {
            return;
        }
        use std::{process::Stdio, time::Duration};
        for mode in ["approval", "command"] {
            let dir = tempfile::tempdir().unwrap();
            let mut caller = crate::test_support::rerun(
                "onboarding::as_root::tests::a_privileged_executor_test_caller",
            )
            .arg("--nocapture")
            .env("ATLAS_EXECUTOR_TEST_DIRECTORY", dir.path())
            .env("ATLAS_EXECUTOR_TEST_MODE", mode)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
            let controller = crate::control::Controller::at(7001, &dir.path().join("state"));
            for _ in 0..500 {
                if dir.path().join("paused").exists() {
                    break;
                }
                if caller.try_wait().unwrap().is_some() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            let paused = dir.path().join("paused").exists();
            let _ = caller.kill();
            let _ = caller.wait();
            let before = controller.pending().unwrap();
            let recovery_blocked = controller.acquire(true).is_err();
            controller.clear().unwrap();
            let after = controller.pending().unwrap();
            let normal_blocked = controller.acquire(false).is_err();
            // Always release the orphaned stand-in, including during a
            // deliberate rollback check, before making assertions.
            fs::write(dir.path().join("continue"), b"").unwrap();
            let pid = fs::read_to_string(dir.path().join("executor"))
                .ok()
                .and_then(|pid| pid.trim().parse::<i32>().ok());
            let mut exited = false;
            for _ in 0..500 {
                if pid.is_none_or(|pid| {
                    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None)
                        == Err(nix::errno::Errno::ESRCH)
                }) {
                    exited = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(
                paused && exited,
                "the stand-in must pause and then exit: {mode}"
            );
            assert!(!before.is_empty());
            assert!(
                recovery_blocked && normal_blocked,
                "a surviving executor blocks competing control: {mode}"
            );
            assert_eq!(
                after, before,
                "recovery preserves executor-backed pending work: {mode}"
            );
            assert!(dir.path().join("acted").exists());
            let held = controller.acquire(true).unwrap();
            controller.clear().unwrap();
            drop(held);
            assert!(controller.acquire(false).is_ok());
        }
    }

    /// A separate caller process for the executor lifetime test. Without
    /// its explicit temporary directory this is an inert test case.
    #[test]
    fn a_privileged_executor_test_caller() {
        use std::{os::fd::OwnedFd, process::Stdio};
        let Some(dir) = std::env::var_os("ATLAS_EXECUTOR_TEST_DIRECTORY") else {
            return;
        };
        let dir = std::path::PathBuf::from(dir);
        let controller = crate::control::Controller::at(7001, &dir.join("state"));
        let script = [EXECUTOR, "print -r -- $$ > \"$4\"; /usr/bin/touch \"$2\"; while [[ ! -e $1 ]]; do /bin/sleep 0.01; done; /usr/bin/touch \"$3\""].concat();
        let args = [
            dir.join("continue"),
            dir.join("paused"),
            dir.join("acted"),
            dir.join("executor"),
        ];
        let _ = super::super::owned(
            &[controller],
            || -> Result<(), Unclosed> {
                if std::env::var("ATLAS_EXECUTOR_TEST_MODE").unwrap() == "command" {
                    let _ = tracked_output(
                        Command::new("/bin/zsh")
                            .args(["-f", "-c", &script, "zsh"])
                            .args(&args),
                    );
                } else {
                    let (ours, theirs) = UnixStream::pair().unwrap();
                    let input: OwnedFd = theirs.try_clone().unwrap().into();
                    let output: OwnedFd = theirs.into();
                    let mut child = Command::new("/bin/zsh")
                        .args(["-f", "-c", &script, "zsh"])
                        .args(&args)
                        .stdin(Stdio::from(input))
                        .stdout(Stdio::from(output))
                        .spawn()
                        .unwrap();
                    let _ = finish_executor(
                        ours,
                        None,
                        Instant::now() + std::time::Duration::from_secs(30),
                    );
                    let _ = child.wait();
                }
                Ok(())
            },
            || true,
        );
    }

    #[test]
    fn a_timed_out_executor_holds_control_and_claims_until_it_exits() {
        if !crate::test_support::isolated(
            "onboarding::as_root::tests::a_timed_out_executor_holds_control_and_claims_until_it_exits",
        ) {
            return;
        }
        use std::{os::fd::OwnedFd, process::Stdio, sync::mpsc, time::Duration};
        let dir = tempfile::tempdir().unwrap();
        let gate = dir.path().join("continue");
        let paused = dir.path().join("paused");
        let acted = dir.path().join("acted");
        let (ours, theirs) = UnixStream::pair().unwrap();
        let input: OwnedFd = theirs.try_clone().unwrap().into();
        let output: OwnedFd = theirs.into();
        let script = [EXECUTOR, "print -r -- '@atlas acting'; /usr/bin/touch \"$2\"; while [[ ! -e $1 ]]; do /bin/sleep 0.01; done; /usr/bin/touch \"$3\""].concat();
        let mut child = Command::new("/bin/zsh")
            .args(["-f", "-c", &script, "zsh"])
            .args([&gate, &paused, &acted])
            .stdin(Stdio::from(input))
            .stdout(Stdio::from(output))
            .spawn()
            .unwrap();
        let state = dir.path().join("state");
        let worker_state = state.clone();
        let (sent, received) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let controller = crate::control::Controller::at(7001, &worker_state);
            let outcome = super::super::owned(
                std::slice::from_ref(&controller),
                || {
                    let (text, ended) = finish_executor(ours, None, Instant::now());
                    match text {
                        Ok(text) => closed_all(&text, &[Target::Orphan(7001)], ended),
                        Err(refusal) => Err(Unclosed {
                            refusal,
                            acted: true,
                            ended,
                        }),
                    }
                },
                || true,
            );
            sent.send(outcome.is_err()).unwrap();
        });
        for _ in 0..200 {
            if paused.exists()
                && crate::control::Controller::at(7001, &state)
                    .pending()
                    .is_ok_and(|pending| !pending.is_empty())
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let controller = crate::control::Controller::at(7001, &state);
        let pending = controller.pending().unwrap();
        let busy = controller.acquire(true).is_err();
        controller.clear().unwrap();
        let after_clear = controller.pending().unwrap();
        let acted_early = acted.exists();
        let early = received.try_recv();
        // Release and reap the child before asserting, including when a
        // deliberately reverted guard allowed an early return.
        fs::write(&gate, b"").unwrap();
        worker.join().unwrap();
        let _ = child.wait();
        assert_eq!(pending.len(), 1);
        assert!(busy, "recovery cannot race the paused executor");
        assert_eq!(after_clear, pending, "recovery cannot erase its claim");
        assert!(!acted_early);
        assert!(
            early.is_err(),
            "the executor still could act when control was released"
        );
        assert!(acted.exists());
        assert!(controller.pending().unwrap().is_empty());
        assert!(controller.acquire(true).is_ok());
    }

    #[test]
    fn terminal_creation_never_runs_while_an_orphan_survives_cleanup() {
        for survives in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let mut tools = login_tools(501, &[]);
            replace(
                &mut tools,
                "/usr/bin/id",
                r#"print -r -- "id $*" >> calls
if [[ $1 == -u && -e created ]]; then print 7003; else exit 1; fi"#,
            );
            tools.push((
                "/usr/sbin/sysadminctl",
                r#"print -r -- "sysadminctl $*" >> calls; touch created"#.into(),
            ));
            let processes = tools
                .iter_mut()
                .find(|(tool, _)| *tool == "/usr/bin/pgrep")
                .unwrap();
            processes.1 = format!("[[ $2 == 7003 ]] && exit 1\n{}", processes.1);
            if survives {
                replace(&mut tools, "/usr/bin/pgrep", "exit 0");
                replace(&mut tools, "/usr/bin/pkill", "exit 0");
            }
            let text = run_steps(
                dir.path(),
                &borrowed(&tools),
                &[
                    "create-terminal",
                    "newhelper",
                    "",
                    "7003",
                    "Atlas",
                    "",
                    "7001",
                ],
            );
            assert_eq!(
                asked(dir.path(), "sysadminctl -addUser"),
                !survives,
                "{text}"
            );
            assert_eq!(dir.path().join("created").exists(), !survives);
            if !survives {
                let calls = calls(dir.path());
                let closed = calls
                    .iter()
                    .position(|line| line == "launchctl bootout user/7001")
                    .unwrap();
                let made = calls
                    .iter()
                    .position(|line| line.starts_with("sysadminctl -addUser"))
                    .unwrap();
                assert!(closed < made);
            }
        }
    }

    #[test]
    fn approved_creation_refuses_an_occupied_uid_or_a_failed_lookup() {
        refuses_occupied_uid("create");
    }

    #[test]
    fn terminal_creation_refuses_an_occupied_uid_or_a_failed_lookup() {
        refuses_occupied_uid("create-terminal");
    }

    fn refuses_occupied_uid(mode: &str) {
        for failure in [
            "collision",
            "garbage",
            "lookup",
            "processes",
            "process-lookup",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let mut tools = login_tools(501, &[]);
            replace(&mut tools, "/usr/bin/id", "exit 1");
            replace(
                &mut tools,
                "/usr/bin/dscl",
                match failure {
                    "collision" => {
                        r#"[[ $1 == /Search ]] && print 'directory-person 7003'; exit 0"#
                    }
                    "garbage" => "print garbage",
                    "lookup" => "exit 3",
                    _ => "exit 0",
                },
            );
            replace(
                &mut tools,
                "/usr/bin/pgrep",
                match failure {
                    "processes" => "exit 0",
                    "process-lookup" => "exit 3",
                    _ => "exit 1",
                },
            );
            tools.push(("/usr/sbin/sysadminctl", "touch forbidden-creation".into()));
            tools.push(("/usr/bin/script", "touch forbidden-creation".into()));
            let text = run_steps(
                dir.path(),
                &borrowed(&tools),
                &[mode, "newhelper", "", "7003", "Atlas", ""],
            );
            assert!(text.contains("@atlas error"), "{mode} {failure}: {text}");
            assert!(
                !dir.path().join("forbidden-creation").exists(),
                "{mode} {failure}: {text}"
            );
        }
    }

    use super::*;
    use crate::setup::helper::Request;
    use std::{fs, process::Command};

    /// The steps with Apple's tools standing in as scripts in `dir` that
    /// `tools` names; the stand-ins share `dir` for notes.
    fn with_stand_ins(dir: &Path, tools: &[(&str, &str)]) -> String {
        let mut script = ["stage_executable() { :; }\n", STEPS].concat();
        for (tool, body) in tools {
            let path = dir.join(Path::new(tool).file_name().unwrap());
            fs::write(
                &path,
                format!("#!/bin/zsh -f\ncd {}\n{body}\n", dir.display()),
            )
            .unwrap();
            fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
            script = script.replace(tool, &path.to_string_lossy());
        }

        script
    }

    /// The steps run as this account, with the stand-ins [`with_stand_ins`]
    /// makes.
    fn run_steps(dir: &Path, tools: &[(&str, &str)], arguments: &[&str]) -> String {
        let script = with_stand_ins(dir, tools);
        run_script(&script, arguments)
    }

    fn run_script(script: &str, arguments: &[&str]) -> String {
        let run = Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", script, "zsh"])
            .args(arguments)
            .output()
            .unwrap();
        String::from_utf8_lossy(&run.stdout).into_owned()
    }

    /// The helper's GeneratedUID in the stand-ins, and another account's.
    const GUID: &str = "6DB4B7AF-BE3C-486C-BD46-1B4558946919";
    const OTHER: &str = "11111111-2222-3333-4444-555555555555";

    /// Stand-ins for the tools that look at and close logins, sharing notes
    /// in their folder:
    /// - the console is `on_screen`, or the user ID in `console` once it is
    ///   there;
    /// - user ID N has an account while it is in `accounts` or `account-N`
    ///   is there;
    /// - every login and process of a user ID runs until launchd boots it
    ///   out (`out-N`) or it is killed (`killed-N`), and its Dock outlives a
    ///   bootout while `dock-N` is there;
    /// - its loginwindow is process 4242, or the one in `window-N`;
    /// - listing processes fails while `broken` is there.
    ///
    /// Each call is noted in `calls`.
    fn login_tools(on_screen: u32, accounts: &[u32]) -> Vec<(&'static str, String)> {
        let accounts: Vec<String> = accounts.iter().map(u32::to_string).collect();
        let accounts = accounts.join(" ");
        vec![
            (
                "/usr/bin/stat",
                format!("[[ -e console ]] && cat console || print {on_screen}"),
            ),
            (
                "/usr/bin/id",
                format!(
                    r#"print -r -- "id $*" >> calls
if [[ $1 == -nu ]]; then [[ " {accounts} " == *" $3 "* || -e account-$3 ]] && print someone || exit 1; fi"#
                ),
            ),
            (
                "/usr/bin/dscl",
                r#"print -r -- "dscl $*" >> calls"#.to_owned(),
            ),
            (
                "/bin/launchctl",
                r#"print -r -- "launchctl $*" >> calls; [[ $1 == bootout ]] && touch out-${2#*/}"#
                    .to_owned(),
            ),
            (
                "/usr/bin/pgrep",
                r#"print -r -- "pgrep $*" >> calls
[[ -e broken ]] && exit 3
case $1 in
  (-qxu) [[ $3 == Dock && -e dock-$2 ]] && exit 0; [[ -e out-$2 ]] && exit 1 || exit 0 ;;
  (-xu) [[ -e out-$2 ]] && exit 1; [[ -e window-$2 ]] && cat window-$2 || print 4242 ;;
  (-qU) [[ -e killed-$2 ]] && exit 1 || exit 0 ;;
esac"#
                    .to_owned(),
            ),
            (
                "/usr/bin/pkill",
                r#"print -r -- "pkill $*" >> calls; touch killed-${@[-1]}"#.to_owned(),
            ),
            ("/bin/sleep", "exit 0".to_owned()),
        ]
    }

    /// [`login_tools`] where user ID `uid` is the standard account `name`
    /// with GeneratedUID [`GUID`] ([`OTHER`] once `reused` is there), until
    /// `deleted` is there. The account was made at 999000, the clock reads
    /// 1000000, and a login has run for what `etime` says, else an hour.
    fn helper_tools(on_screen: u32, uid: u32, name: &str) -> Vec<(&'static str, String)> {
        let mut tools = login_tools(on_screen, &[]);
        replace(
            &mut tools,
            "/usr/bin/id",
            format!(
                r#"print -r -- "id $*" >> calls
case $1 in
  (-nu) [[ $3 == {uid} && ! -e deleted ]] && print {name} && exit 0
        [[ -e account-$3 ]] && print someone || exit 1 ;;
  (-u) [[ $3 == {name} && ! -e deleted ]] && print {uid} || exit 1 ;;
esac"#
            ),
        );
        replace(
            &mut tools,
            "/usr/bin/dscl",
            format!(
                r#"print -r -- "dscl $*" >> calls
case $4 in
  (GeneratedUID) [[ -e reused ]] && print "GeneratedUID: {OTHER}" || print "GeneratedUID: {GUID}" ;;
  (accountPolicyData) print "dsAttrTypeNative:accountPolicyData:"; print "<dict><key>creationTime</key>"; print "	<real>999000.25</real></dict>" ;;
esac"#
            ),
        );
        tools.push(("/usr/sbin/dseditgroup", "exit 67".to_owned()));
        tools.push((
            "/bin/ps",
            "[[ -e etime ]] && cat etime || print '  01:00:00'".to_owned(),
        ));
        tools.push(("/bin/date", "print 1000000".to_owned()));
        tools.push((
            "/usr/sbin/sysadminctl",
            r#"print -r -- "sysadminctl $*" >> calls; touch deleted"#.to_owned(),
        ));
        tools
    }

    /// Put `body` in place of the stand-in for `tool`.
    fn replace(tools: &mut [(&'static str, String)], tool: &str, body: impl Into<String>) {
        let body = body.into();
        let found = tools
            .iter_mut()
            .find(|(name, _)| *name == tool)
            .unwrap_or_else(|| panic!("no stand-in for {tool}"));
        found.1 = body;
    }

    /// What the stand-ins were asked, in order.
    fn calls(dir: &Path) -> Vec<String> {
        fs::read_to_string(dir.join("calls"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Whether any call the stand-ins were asked starts with `start`.
    fn asked(dir: &Path, start: &str) -> bool {
        calls(dir).iter().any(|call| call.starts_with(start))
    }

    fn borrowed<'a>(tools: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
        tools
            .iter()
            .map(|(tool, body)| (*tool, body.as_str()))
            .collect()
    }

    fn me() -> u32 {
        nix::unistd::getuid().as_raw()
    }

    /// The helper `name` with user ID `uid`, as the stand-ins have it.
    fn identity(uid: u32, name: &str) -> Identity {
        Identity {
            uid,
            name: name.to_owned(),
            generated_uid: GUID.to_owned(),
        }
    }

    #[test]
    fn closing_refuses_the_screen_the_asker_an_account_and_a_system_user_id() {
        // A user ID other than this account's and the console's.
        let free = if me() == 7001 { 7002 } else { 7001 };
        let cases = [
            (free, vec![], free, format!("user ID {free} is on screen")),
            (
                me(),
                vec![],
                501,
                format!("user ID {} is the account asking", me()),
            ),
            (
                free,
                vec![free],
                501,
                format!("user ID {free} has an account"),
            ),
            (
                500,
                vec![],
                501,
                "user ID 500 is not a person's account".to_owned(),
            ),
        ];

        for (uid, accounts, on_screen, why) in cases {
            let dir = tempfile::tempdir().unwrap();
            let tools = login_tools(on_screen, &accounts);
            let target = Target::Orphan(uid).argument();
            let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
            assert_eq!(closed(&text), Err(Refusal::Failed(why)), "{text}");
            assert!(
                !asked(dir.path(), "launchctl") && !asked(dir.path(), "pkill"),
                "nothing closed for {uid}"
            );
        }
    }

    #[test]
    fn a_network_account_found_only_by_searching_every_directory_is_not_closed() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        replace(
            &mut tools,
            "/usr/bin/dscl",
            "print 'network\t\tUniqueID = (\n    7001\n)'",
        );
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed("user ID 7001 has an account".into()))
        );
    }

    #[test]
    fn every_user_id_is_checked_before_any_login_is_closed() {
        let dir = tempfile::tempdir().unwrap();
        let tools = login_tools(501, &[7002]);
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["close", "orphan:7001", "orphan:7002"],
        );
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed("user ID 7002 has an account".into()))
        );
        assert!(!asked(dir.path(), "launchctl"));
    }

    #[test]
    fn a_target_the_steps_do_not_know_is_refused_before_anything_is_closed() {
        let dir = tempfile::tempdir().unwrap();
        let tools = helper_tools(501, 7004, "opie");
        for target in [
            "7001",
            "orphan:7001:x",
            "helper:7004:opie",
            "leftover:7004::GUID:1",
        ] {
            let text = run_steps(
                dir.path(),
                &borrowed(&tools),
                &["close", "orphan:7003", target],
            );
            assert!(closed(&text).is_err(), "{target}: {text}");
        }
        assert!(!asked(dir.path(), "launchctl"));
    }

    #[test]
    fn launchd_ends_a_deleted_accounts_login_before_anything_is_killed() {
        let dir = tempfile::tempdir().unwrap();
        let tools = login_tools(501, &[]);
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["close", "orphan:7001", "orphan:7003"],
        );
        assert_eq!(closed(&text), Ok(vec![7001, 7003]), "{text}");

        let calls = calls(dir.path());
        let at = |wanted: &str| {
            calls
                .iter()
                .position(|call| call == wanted)
                .unwrap_or_else(|| panic!("no {wanted} in {calls:?}"))
        };
        assert!(at("launchctl bootout user/7001") < at("pkill -TERM -U 7001"));
        assert!(at("pkill -TERM -U 7001") < at("launchctl bootout user/7003"));
        assert!(
            calls.iter().all(|call| !call.contains("KILL")),
            "stopped at the first ask"
        );
    }

    #[test]
    fn a_login_that_outlives_its_stop_is_killed_and_one_that_outlives_that_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        // Only a kill ends what is left.
        replace(
            &mut tools,
            "/usr/bin/pkill",
            r#"print -r -- "pkill $*" >> calls; [[ $1 == -KILL ]] && touch killed-${@[-1]}"#,
        );
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert_eq!(closed(&text), Ok(vec![7001]));
        assert!(calls(dir.path()).contains(&"pkill -KILL -U 7001".to_owned()));

        let dir = tempfile::tempdir().unwrap();
        replace(
            &mut tools,
            "/usr/bin/pkill",
            r#"print -r -- "pkill $*" >> calls"#,
        );
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed(
                "macOS didn't close the login of user ID 7001".into()
            ))
        );
    }

    #[test]
    fn an_account_made_for_a_later_user_id_stops_the_steps_before_its_bootout() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        // While 7001 closes, an account is made with user ID 7003.
        replace(
            &mut tools,
            "/bin/launchctl",
            r#"print -r -- "launchctl $*" >> calls; touch out-${2#*/} account-7003"#,
        );
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["close", "orphan:7001", "orphan:7003"],
        );
        assert!(text.contains("@atlas closed 7001"), "{text}");
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed("user ID 7003 has an account".into()))
        );
        assert!(!calls(dir.path()).contains(&"launchctl bootout user/7003".to_owned()));
    }

    #[test]
    fn an_account_made_during_the_escalation_stops_the_next_kill() {
        // Made once launchd booted the login out: nothing is stopped.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        replace(
            &mut tools,
            "/bin/launchctl",
            r#"print -r -- "launchctl $*" >> calls; touch account-7001"#,
        );
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed("user ID 7001 has an account".into()))
        );
        assert!(!asked(dir.path(), "pkill"));

        // Made once its processes were asked to stop: nothing is killed.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        replace(
            &mut tools,
            "/usr/bin/pkill",
            r#"print -r -- "pkill $*" >> calls; touch account-7001"#,
        );
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed("user ID 7001 has an account".into()))
        );
        assert!(calls(dir.path()).contains(&"pkill -TERM -U 7001".to_owned()));
        assert!(!calls(dir.path()).contains(&"pkill -KILL -U 7001".to_owned()));
    }

    #[test]
    fn a_login_that_comes_on_screen_is_never_killed_or_booted_out_again() {
        // A deleted account's login, on screen once launchd was asked.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        replace(
            &mut tools,
            "/bin/launchctl",
            r#"print -r -- "launchctl $*" >> calls; print 7001 > console"#,
        );
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed("user ID 7001 is on screen".into()))
        );
        assert!(!asked(dir.path(), "pkill"));

        // The helper, on screen once its GUI session was booted out.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = helper_tools(501, 7004, "opie");
        replace(
            &mut tools,
            "/bin/launchctl",
            r#"print -r -- "launchctl $*" >> calls; print 7004 > console"#,
        );
        let target = Target::Helper(identity(7004, "opie")).argument();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed(
                "the helper is on screen; switch back to your own account first".into()
            ))
        );
        assert!(!calls(dir.path()).contains(&"launchctl bootout user/7004".to_owned()));
    }

    #[test]
    fn who_is_on_screen_is_read_after_every_lookup_right_before_each_step() {
        // On screen while root looks the deleted account up a last time.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[]);
        replace(
            &mut tools,
            "/usr/bin/dscl",
            r#"print -r -- "dscl $*" >> calls
print x >> looked; (( $(wc -l < looked) == 4 )) && print 7001 > console; exit 0"#,
        );
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed("user ID 7001 is on screen".into()))
        );
        assert!(!asked(dir.path(), "launchctl"));

        // The helper, on screen while root checks its account a last time.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = helper_tools(501, 7004, "opie");
        replace(
            &mut tools,
            "/usr/sbin/dseditgroup",
            "print x >> looked; (( $(wc -l < looked) == 2 )) && print 7004 > console; exit 67",
        );
        let target = Target::Helper(identity(7004, "opie")).argument();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed(
                "the helper is on screen; switch back to your own account first".into()
            ))
        );
        assert!(!asked(dir.path(), "launchctl"));
    }

    #[test]
    fn a_login_that_starts_during_the_last_check_stops_the_deletion() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("out-7005"), "").unwrap();
        let mut tools = helper_tools(501, 7005, "opie");
        // Logged in again while root checks the account the second time.
        replace(
            &mut tools,
            "/usr/sbin/dseditgroup",
            "print x >> looked; (( $(wc -l < looked) == 2 )) && rm -f out-7005; exit 67",
        );
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["remove-helper", "7005", "opie", GUID],
        );
        assert_eq!(
            refused(&text),
            Err(Refusal::Failed("the helper is still logged in".into()))
        );
        assert!(!asked(dir.path(), "sysadminctl"));
    }

    #[test]
    fn the_helpers_own_login_is_ended_through_launchd_and_never_killed() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = helper_tools(501, 7004, "opie");
        // The first bootout leaves it running.
        replace(
            &mut tools,
            "/bin/launchctl",
            r#"print -r -- "launchctl $*" >> calls; [[ $2 == user/* ]] && touch out-${2#*/}"#,
        );
        let target = Target::Helper(identity(7004, "opie")).argument();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(closed(&text), Ok(vec![7004]), "{text}");

        let calls = calls(dir.path());
        let gui = calls
            .iter()
            .position(|call| call == "launchctl bootout gui/7004");
        let user = calls
            .iter()
            .position(|call| call == "launchctl bootout user/7004");
        assert!(gui.is_some() && gui < user, "{calls:?}");
        assert!(calls.iter().all(|call| !call.starts_with("pkill")));
    }

    #[test]
    fn the_helpers_login_is_never_closed_on_screen_or_as_an_administrator() {
        let target = Target::Helper(identity(7004, "opie")).argument();
        let dir = tempfile::tempdir().unwrap();
        let tools = helper_tools(7004, 7004, "opie");
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed(
                "the helper is on screen; switch back to your own account first".into()
            ))
        );

        let dir = tempfile::tempdir().unwrap();
        let mut tools = helper_tools(501, 7004, "opie");
        replace(&mut tools, "/usr/sbin/dseditgroup", "exit 0");
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed(
                "user ID 7004 is an administrator, or macOS could not tell".into()
            ))
        );
        assert!(!asked(dir.path(), "launchctl"));
    }

    #[test]
    fn a_user_id_given_to_another_account_is_never_closed_or_deleted() {
        let no_longer = || Refusal::Failed("user ID 7004 is no longer the helper's account".into());
        let tools = helper_tools(501, 7004, "opie");
        let target = Target::Helper(identity(7004, "opie")).argument();

        // Another account had the user ID before the steps ran.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("reused"), "").unwrap();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(closed(&text), Err(no_longer()));
        fs::write(dir.path().join("out-7004"), "").unwrap();
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["remove-helper", "7004", "opie", GUID],
        );
        assert_eq!(refused(&text), Err(no_longer()));
        assert!(!asked(dir.path(), "launchctl") && !asked(dir.path(), "sysadminctl"));

        // A name that is not the account's own.
        let dir = tempfile::tempdir().unwrap();
        let named = Target::Helper(identity(7004, "ann")).argument();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &named]);
        assert_eq!(closed(&text), Err(no_longer()));

        // Another account took it while launchd ended the GUI session.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = helper_tools(501, 7004, "opie");
        replace(
            &mut tools,
            "/bin/launchctl",
            r#"print -r -- "launchctl $*" >> calls; touch reused"#,
        );
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(closed(&text), Err(no_longer()));
        assert!(calls(dir.path()).contains(&"launchctl bootout gui/7004".to_owned()));
        assert!(!calls(dir.path()).contains(&"launchctl bootout user/7004".to_owned()));
    }

    #[test]
    fn a_leftover_login_is_closed_only_while_it_is_the_same_login_older_than_the_account() {
        let tools = helper_tools(501, 7004, "opie");
        let target = Target::Leftover {
            helper: identity(7004, "opie"),
            pid: 4242,
        }
        .argument();

        // An hour old, the account under twenty minutes: closed by launchd.
        let dir = tempfile::tempdir().unwrap();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(closed(&text), Ok(vec![7004]), "{text}");
        assert!(calls(dir.path()).contains(&"launchctl bootout gui/7004".to_owned()));
        assert!(!asked(dir.path(), "pkill"));

        // Another login of the user ID now.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("window-7004"), "5151\n").unwrap();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed("user ID 7004 has another login now".into()))
        );
        assert!(!asked(dir.path(), "launchctl"));

        // A login newer than the account: the helper's own.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("etime"), "   00:30\n").unwrap();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed(
                "the login of user ID 7004 is the helper's own".into()
            ))
        );
        assert!(!asked(dir.path(), "launchctl"));
    }

    #[test]
    fn every_kind_of_login_closes_in_one_run() {
        let dir = tempfile::tempdir().unwrap();
        let tools = helper_tools(501, 7004, "opie");
        let leftover = Target::Leftover {
            helper: identity(7004, "opie"),
            pid: 4242,
        }
        .argument();
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["close", "orphan:7001", &leftover],
        );
        assert_eq!(closed(&text), Ok(vec![7001, 7004]), "{text}");
        assert!(calls(dir.path()).contains(&"pkill -TERM -U 7001".to_owned()));
        assert!(!calls(dir.path()).contains(&"pkill -TERM -U 7004".to_owned()));
    }

    #[test]
    fn the_helper_is_deleted_only_once_logged_out_and_only_then_is_anything_killed() {
        let tools = helper_tools(501, 7005, "opie");
        let arguments = ["remove-helper", "7005", "opie", GUID];

        // Still logged in: nothing is deleted.
        let dir = tempfile::tempdir().unwrap();
        let text = run_steps(dir.path(), &borrowed(&tools), &arguments);
        assert!(
            text.contains("@atlas error the helper is still logged in"),
            "{text}"
        );
        assert!(!asked(dir.path(), "sysadminctl"));

        // Logged out: deleted, then what macOS left running is closed.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("out-7005"), "").unwrap();
        let text = run_steps(dir.path(), &borrowed(&tools), &arguments);
        assert!(text.contains("@atlas removed 7005"), "{text}");

        let calls = calls(dir.path());
        let deleted = calls
            .iter()
            .position(|call| call == "sysadminctl -deleteUser opie")
            .unwrap();
        let killed = calls
            .iter()
            .position(|call| call == "pkill -TERM -U 7005")
            .unwrap();
        assert!(deleted < killed, "{calls:?}");
    }

    #[test]
    fn a_dock_left_running_is_not_logged_out() {
        // Its loginwindow ended, its Dock not: nothing is deleted.
        let dir = tempfile::tempdir().unwrap();
        let tools = helper_tools(501, 7005, "opie");
        fs::write(dir.path().join("out-7005"), "").unwrap();
        fs::write(dir.path().join("dock-7005"), "").unwrap();
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["remove-helper", "7005", "opie", GUID],
        );
        assert_eq!(
            refused(&text),
            Err(Refusal::Failed("the helper is still logged in".into()))
        );
        assert!(!asked(dir.path(), "sysadminctl"));

        // Ending the helper's login leaves its Dock: not ended.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("dock-7005"), "").unwrap();
        let target = Target::Helper(identity(7005, "opie")).argument();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", &target]);
        assert_eq!(
            closed(&text),
            Err(Refusal::Failed(
                "macOS didn't end the login of user ID 7005".into()
            ))
        );
        assert!(!asked(dir.path(), "pkill"));
    }

    #[test]
    fn a_process_list_that_fails_is_never_taken_for_none() {
        let failed = |uid| {
            Refusal::Failed(format!(
                "macOS could not list the processes of user ID {uid}"
            ))
        };
        let tools = helper_tools(501, 7005, "opie");

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("broken"), "").unwrap();
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["remove-helper", "7005", "opie", GUID],
        );
        assert_eq!(refused(&text), Err(failed(7005)));
        assert!(!asked(dir.path(), "sysadminctl"));

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("broken"), "").unwrap();
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert_eq!(closed(&text), Err(failed(7001)));
        assert!(!asked(dir.path(), "pkill"));
    }

    #[test]
    fn a_refusal_before_root_acts_is_told_from_one_after() {
        let one = [Target::Orphan(7001)];
        assert_eq!(
            closed_all("@atlas error user ID 7001 is on screen\n", &one, true),
            Err(Unclosed {
                refusal: Refusal::Failed("user ID 7001 is on screen".into()),
                acted: false,
                ended: true,
            })
        );
        let late = "@atlas acting\n@atlas error macOS didn't close the login of user ID 7001\n";
        let late = closed_all(late, &one, true).unwrap_err();
        assert!(late.acted && late.ended);
        let done = "@atlas acting\n@atlas closed 7001\n@atlas done\n";
        assert_eq!(closed_all(done, &one, true), Ok(vec![7001]));
        assert!(
            !closed_all(done, &one, false).unwrap_err().ended,
            "output alone is not termination"
        );
        let two = [Target::Orphan(7001), Target::Orphan(7003)];
        assert!(
            closed_all(done, &two, true).unwrap_err().acted,
            "one never closed"
        );

        // Cut short before its last line: it may still be acting.
        let short = closed_all("@atlas acting\n@atlas closed 7001\n", &one, false).unwrap_err();
        assert!(short.acted && !short.ended);

        // The same reply from a run seen to exit afterwards: it ended.
        assert_eq!(
            closed_all("@atlas acting\n@atlas closed 7001\n", &one, true),
            Ok(vec![7001])
        );

        // The steps say so themselves.
        let dir = tempfile::tempdir().unwrap();
        let tools = login_tools(7001, &[]);
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert!(!closed_all(&text, &one, true).unwrap_err().acted, "{text}");
        let dir = tempfile::tempdir().unwrap();
        let tools = login_tools(501, &[]);
        let text = run_steps(dir.path(), &borrowed(&tools), &["close", "orphan:7001"]);
        assert!(text.contains("@atlas acting"), "{text}");
        assert!(text.trim_end().ends_with("@atlas done"), "{text}");
    }

    #[test]
    fn a_setup_run_is_told_untouched_from_one_that_may_have_acted() {
        assert_eq!(
            readied((Err(Refusal::Cancelled), true)),
            Err(untouched(Refusal::Cancelled))
        );
        assert_eq!(
            readied((Ok("@atlas readied {\"ok\": true}".into()), true)),
            Ok(())
        );

        assert!(
            !readied((Ok("@atlas readied {\"ok\": true}".into()), false))
                .unwrap_err()
                .ended
        );

        let refused = readied((
            Ok("@atlas closed 7001\n@atlas error opie already exists\n".into()),
            true,
        ))
        .unwrap_err();
        assert!(refused.acted && refused.ended, "its executor exited");

        let lost = readied((
            Err(Refusal::Failed(
                "Atlas stopped waiting for the approved step, which may still finish".into(),
            )),
            false,
        ))
        .unwrap_err();
        assert!(lost.acted && !lost.ended);
    }

    #[test]
    fn targets_are_named_as_the_steps_read_them() {
        assert_eq!(Target::Orphan(7001).argument(), "orphan:7001");
        assert_eq!(
            Target::Helper(identity(502, "opie")).argument(),
            format!("helper:502:opie:{GUID}")
        );
        let leftover = Target::Leftover {
            helper: identity(502, "opie"),
            pid: 77,
        };
        assert_eq!(leftover.argument(), format!("leftover:502:opie:{GUID}:77"));
        assert_eq!(leftover.uid(), 502);
    }

    #[test]
    fn readying_closes_deleted_accounts_logins_first() {
        let dir = tempfile::tempdir().unwrap();
        // The helper itself is logged out.
        fs::write(dir.path().join("out-502"), "").unwrap();
        let mut tools = login_tools(501, &[502]);
        replace(
            &mut tools,
            "/usr/bin/id",
            r#"print -r -- "id $*" >> calls
case $1 in
  (-nu) [[ $3 == 502 ]] && print opie || exit 1 ;;
  (-u) print 502 ;;
esac"#,
        );
        replace(&mut tools, "/usr/bin/dscl", GUID_TOOL);
        tools.push(("/usr/sbin/dseditgroup", "exit 67".to_owned()));
        tools.push(("/usr/sbin/createhomedir", "exit 0".to_owned()));
        tools.push((
            "/usr/bin/sudo",
            r#"print -r -- "sudo $*" >> calls; print -r -- '{"ok": true}'"#.to_owned(),
        ));
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["choose", "opie", EXECUTABLE, "502", GUID, "7001"],
        );
        assert_eq!(outcome(&text), Ok(()), "{text}");
        assert!(text.find("@atlas closed 7001") < text.find("@atlas readied"));

        let calls = calls(dir.path());
        let closed = calls
            .iter()
            .position(|call| call == "pkill -TERM -U 7001")
            .unwrap();
        let readied = calls
            .iter()
            .position(|call| call.starts_with("sudo"))
            .unwrap();
        assert!(closed < readied, "{calls:?}");
    }

    #[test]
    fn a_login_that_cannot_be_closed_stops_the_creation() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = login_tools(501, &[7001]);
        tools.push((
            "/usr/sbin/sysadminctl",
            r#"print -r -- "sysadminctl $*" >> calls"#.to_owned(),
        ));
        let text = run_steps(
            dir.path(),
            &borrowed(&tools),
            &["create", "opie", EXECUTABLE, "502", "Opie", "", "7001"],
        );
        assert_eq!(
            outcome(&text),
            Err(Refusal::Failed("user ID 7001 has an account".into()))
        );
        assert!(!asked(dir.path(), "sysadminctl"));
    }

    #[test]
    fn closing_is_read_from_its_own_lines() {
        assert_eq!(
            closed("@atlas closed 502\n@atlas closed 503\n"),
            Ok(vec![502, 503])
        );
        assert_eq!(
            closed("@atlas closed 502\n@atlas error macOS didn't close the login of user ID 503\n"),
            Err(Refusal::Failed(
                "macOS didn't close the login of user ID 503".into()
            ))
        );
        assert_eq!(
            closed(""),
            Err(Refusal::Failed("The step did not finish".into()))
        );
        assert_eq!(
            outcome("@atlas closed 7001\n@atlas readied {\"ok\": true}\n"),
            Ok(())
        );
        assert_eq!(
            outcome("@atlas closed 7001\n@atlas error opie already exists\n"),
            Err(Refusal::Failed("opie already exists".into()))
        );
    }

    /// A stand-in for dscl: the helper's GeneratedUID ([`OTHER`] once
    /// `reused` is there), and no account found by any search.
    const GUID_TOOL: &str = r#"print -r -- "dscl $*" >> calls
if [[ $4 == GeneratedUID ]]; then
  [[ -e reused ]] && print "GeneratedUID: 11111111-2222-3333-4444-555555555555" || print "GeneratedUID: 6DB4B7AF-BE3C-486C-BD46-1B4558946919"
fi
exit 0"#;

    /// A stand-in for id: opie has user ID 502 (777 once `moved` is there).
    const OPIE: &str = r#"print -r -- "id $*" >> calls
case $1 in
  (-nu) [[ $3 == 502 && ! -e moved ]] && print opie || exit 1 ;;
  (-u) [[ -e moved ]] && print 777 || print 502 ;;
esac"#;

    const STANDARD: [(&str, &str); 5] = [
        ("/usr/sbin/dseditgroup", "exit 67"),
        (
            "/usr/sbin/createhomedir",
            r#"print -r -- "createhomedir $*" >> calls"#,
        ),
        ("/usr/bin/pgrep", "exit 1"),
        (
            "/usr/bin/sudo",
            r#"print -rl -- "$@" > sudo.log; print -r -- '{"ok": true}'"#,
        ),
        ("/usr/bin/dscl", GUID_TOOL),
    ];

    const EXECUTABLE: &str = "/Applications/My Atlas $x *.app/Contents/MacOS/atlas";

    #[test]
    fn each_outcome_is_read_from_its_own_line() {
        let readied = "@atlas readied {\n  \"ok\": true,\n  \"answered\": []\n}\n";
        assert_eq!(outcome(readied), Ok(()));
        assert_eq!(outcome(&format!("@atlas created 1\n{readied}")), Ok(()));

        let refused = "User password:\r\n\
                       New account password error (eDSAuthPasswordTooShort)\r\n\
                       @atlas created 1\n@atlas error macOS did not create opie\n";
        assert_eq!(
            outcome(refused),
            Err(Refusal::Failed(
                "New account password error (eDSAuthPasswordTooShort)".into()
            ))
        );
        assert_eq!(
            outcome("@atlas error opie is an administrator, or macOS could not tell\n"),
            Err(Refusal::Failed(
                "opie is an administrator, or macOS could not tell".into()
            ))
        );
        assert_eq!(
            outcome("@atlas readied {\"ok\": false, \"error\": \"has Atlas settings\"}"),
            Err(Refusal::Failed("has Atlas settings".into()))
        );
        assert_eq!(
            outcome("@atlas readied sudo: unknown user opie"),
            Err(Refusal::Failed("sudo: unknown user opie".into()))
        );
        assert!(outcome("@atlas created 0\n").is_err(), "it never finished");

        let joined =
            "eDSRecordAlreadyExists\r@atlas created 72\n@atlas error macOS did not create opie\n";
        assert_eq!(
            outcome(joined),
            Err(Refusal::Failed("eDSRecordAlreadyExists".into())),
            "sysadminctl's last words without a newline"
        );
        assert_eq!(
            outcome(
                "User password:\r\n\n@atlas created 72\n@atlas error macOS did not create opie\n"
            ),
            Err(Refusal::Failed(
                "macOS did not create opie (sysadminctl stopped with status 72)".into()
            )),
            "nothing said but its status"
        );
    }

    #[test]
    fn a_password_that_appears_in_the_steps_lines_leaves_them_whole() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let writer = std::thread::spawn(move || {
            let mut theirs = theirs;
            std::io::Write::write_all(
                &mut theirs,
                b"atlas\r\n@atlas created 0\n@atlas readied {\"ok\": true}\n",
            )
            .unwrap();
        });
        let (finished, text) = converse(ours, Some(b"atlas"));
        let text = text.unwrap();
        assert!(finished);
        writer.join().unwrap();
        assert!(!text.starts_with("atlas"), "cut from sysadminctl's part");
        assert_eq!(outcome(&text), Ok(()));
    }

    #[test]
    fn readying_asks_the_helper_for_its_first_login_as_setup_helper_reads_it() {
        let request = serde_json::to_string(&Request::FirstLogin {
            uid: 502,
            opener: true,
        })
        .unwrap();
        let written = request.replace('"', "\\\"").replace("502", "$uid");
        assert!(
            STEPS.contains(&format!("request=\"{written}\"")),
            "{written}"
        );
        assert!(STEPS.contains(&format!("<{}-", crate::setup::FIRST_USER_UID)));
    }

    #[test]
    fn the_steps_are_valid_zsh_and_stop_without_an_administrator() {
        let script = [BECOME_ROOT, &steps()].concat();
        let check = Command::new("/bin/zsh")
            .args(["-n", "-f", "-c", &script])
            .output()
            .unwrap();
        assert!(
            check.status.success(),
            "{}",
            String::from_utf8_lossy(&check.stderr)
        );

        let run = Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args(["choose", "nobody", "/nowhere", "501"])
            .output()
            .unwrap();
        assert_eq!(
            outcome(&String::from_utf8_lossy(&run.stdout)),
            Err(Refusal::Failed(
                "macOS did not let Atlas act as an administrator".into()
            ))
        );
    }

    #[test]
    fn the_worker_runs_the_verified_copy_even_when_the_source_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Source.app/Contents/MacOS");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("atlas"), "verified").unwrap();
        fs::write(source.join("AtlasApp"), "verified app").unwrap();
        let tools = [
            ("/usr/bin/id", OPIE),
            ("/usr/sbin/chown", "print -rl -- \"$@\" > ownership"),
            (
                "/usr/bin/codesign",
                r#"target=${@[-1]}
[[ $(/usr/bin/stat -f %Lp ${target%/Atlas.app*}) == 700 ]] || exit 1
if [[ $target == *.app ]]; then
  [[ $(<$target/Contents/MacOS/atlas) == verified ]] || exit 1
  print replaced > Source.app/Contents/MacOS/atlas
fi
print -r -- "$*" >> verification"#,
            ),
            (
                "/usr/bin/sudo",
                r#"print -r -- "$4" > worker-path
cat "$4" > worker-bytes
print -r -- '{"ok": true}'"#,
            ),
        ];
        let mut tools = tools.to_vec();
        tools.extend(
            STANDARD
                .into_iter()
                .filter(|(tool, _)| !matches!(*tool, "/usr/bin/sudo")),
        );
        let script = staged_stand_ins(dir.path(), &tools);
        let text = run_script(
            &script,
            &[
                "choose",
                "opie",
                source.join("atlas").to_str().unwrap(),
                "502",
                GUID,
            ],
        );
        assert_eq!(outcome(&text), Ok(()), "{text}");
        assert_eq!(
            fs::read_to_string(dir.path().join("worker-bytes")).unwrap(),
            "verified"
        );
        assert_eq!(
            fs::read_to_string(source.join("atlas")).unwrap(),
            "replaced\n"
        );
        let worker = fs::read_to_string(dir.path().join("worker-path")).unwrap();
        assert!(worker.starts_with(dir.path().to_str().unwrap()));
        assert!(worker.contains("/atlas-approved."));
        assert!(!Path::new(worker.trim()).exists());
        assert_no_staged_copies(dir.path());
        let ownership = fs::read_to_string(dir.path().join("ownership")).unwrap();
        assert!(ownership.starts_with("-R\n0:0\n"));
        let verified = fs::read_to_string(dir.path().join("verification")).unwrap();
        assert!(verified.contains(&crate::setup::requirement_for(crate::setup::IDENTIFIER)));
        assert!(verified.contains(&crate::setup::requirement_for(crate::setup::APP_IDENTIFIER)));
    }

    #[test]
    fn every_failure_after_staging_removes_the_copy() {
        for failure in [
            "permissions",
            "copy",
            "ownership",
            "opening",
            "account",
            "worker",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("Source.app/Contents/MacOS");
            fs::create_dir_all(&source).unwrap();
            fs::write(source.join("atlas"), "verified").unwrap();
            fs::write(source.join("AtlasApp"), "verified app").unwrap();
            let mut tools = vec![
                ("/usr/bin/id", OPIE),
                ("/usr/sbin/chown", "exit 0"),
                ("/usr/bin/codesign", "exit 0"),
            ];
            tools.extend(STANDARD);
            match failure {
                "permissions" => tools.push(("/bin/chmod", "exit 1")),
                "copy" => tools.push(("/usr/bin/ditto", "exit 1")),
                "ownership" => tools[1].1 = "exit 1",
                "opening" => {
                    tools.push(("/bin/chmod", "[[ $1 == 755 ]] && exit 1; /bin/chmod \"$@\""))
                }
                "account" => tools[0].1 = "exit 1",
                "worker" => {
                    let sudo = tools
                        .iter_mut()
                        .find(|(tool, _)| *tool == "/usr/bin/sudo")
                        .unwrap();
                    sudo.1 = "print -r -- '{\"ok\":false,\"error\":\"worker failed\"}'; exit 1";
                }
                _ => unreachable!(),
            }
            let script = staged_stand_ins(dir.path(), &tools);
            let text = run_script(
                &script,
                &[
                    "choose",
                    "opie",
                    source.join("atlas").to_str().unwrap(),
                    "502",
                    GUID,
                ],
            );
            assert!(outcome(&text).is_err(), "{failure}: {text}");
            assert_no_staged_copies(dir.path());
        }
    }

    #[test]
    fn a_closed_reply_pipe_after_staging_still_removes_the_copy() {
        use std::{
            io::{BufRead, BufReader, Write},
            process::Stdio,
        };

        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Source.app/Contents/MacOS");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("atlas"), "verified").unwrap();
        fs::write(source.join("AtlasApp"), "verified app").unwrap();
        let script = staged_stand_ins(
            dir.path(),
            &[
                ("/usr/bin/id", "exit 1"),
                ("/usr/sbin/chown", "exit 0"),
                ("/usr/bin/codesign", "exit 0"),
                (
                    "/bin/chmod",
                    r#"if [[ $1 == 755 ]]; then
  print -r -- "$2"
  IFS= read -r acknowledgement || exit 1
fi
/bin/chmod "$@""#,
                ),
            ],
        );
        let mut child = Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args([
                "choose",
                "opie",
                source.join("atlas").to_str().unwrap(),
                "502",
                GUID,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut reply = BufReader::new(child.stdout.take().unwrap());
        let mut staged = String::new();
        reply.read_line(&mut staged).unwrap();
        assert!(Path::new(staged.trim()).join("Atlas.app").exists());
        drop(reply);
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"continue\n")
            .unwrap();
        let status = child.wait().unwrap();
        assert_no_staged_copies(dir.path());
        assert_eq!(status.code(), Some(1));
    }

    fn assert_no_staged_copies(dir: &Path) {
        assert!(!fs::read_dir(dir).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("atlas-approved.")
        }));
    }

    #[test]
    fn an_invalid_or_linked_copy_never_reaches_the_worker() {
        for linked in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("Source.app/Contents/MacOS");
            fs::create_dir_all(&source).unwrap();
            fs::write(source.join("atlas"), "replacement").unwrap();
            if linked {
                std::os::unix::fs::symlink("/bin/ls", source.join("AtlasApp")).unwrap();
            } else {
                fs::write(source.join("AtlasApp"), "replacement app").unwrap();
            }
            let mut tools = vec![
                ("/usr/bin/id", OPIE),
                ("/usr/sbin/chown", "exit 0"),
                ("/usr/bin/codesign", "exit 1"),
            ];
            tools.extend(STANDARD);
            let script = staged_stand_ins(dir.path(), &tools);
            let text = run_script(
                &script,
                &[
                    "choose",
                    "opie",
                    source.join("atlas").to_str().unwrap(),
                    "502",
                    GUID,
                ],
            );
            if linked {
                assert_eq!(
                    outcome(&text),
                    Err(Refusal::Failed(
                        "Atlas's copy contains links or special files".into()
                    )),
                    "{text}"
                );
            } else {
                assert!(outcome(&text).is_err(), "{text}");
            }
            assert!(!dir.path().join("sudo.log").exists());
            assert!(!asked(dir.path(), "createhomedir"));
            assert_no_staged_copies(dir.path());
        }
    }

    fn staged_stand_ins(dir: &Path, tools: &[(&str, &str)]) -> String {
        let staging = steps().strip_suffix(STEPS).unwrap().to_owned();
        let mut script =
            with_stand_ins(dir, tools).replacen("stage_executable() { :; }\n", &staging, 1);
        // Staging also uses tool stand-ins, and all copies stay in the fixture.
        for (tool, _) in tools {
            script = script.replace(
                tool,
                &dir.join(Path::new(tool).file_name().unwrap())
                    .to_string_lossy(),
            );
        }
        script.replace(
            "/private/var/tmp/atlas-approved.XXXXXXXX",
            &format!("{}/atlas-approved.XXXXXXXX", dir.display()),
        )
    }

    #[test]
    fn readying_hands_names_and_paths_to_each_tool_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = vec![("/usr/bin/id", OPIE)];
        tools.extend(STANDARD);
        let text = run_steps(
            dir.path(),
            &tools,
            &["choose", "opie", EXECUTABLE, "502", GUID],
        );

        assert_eq!(outcome(&text), Ok(()), "{text}");
        assert_eq!(
            fs::read_to_string(dir.path().join("sudo.log"))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            [
                "-n",
                "-u",
                "opie",
                EXECUTABLE,
                "setup-helper",
                r#"{"action":"first-login","uid":502,"opener":true}"#,
                "--account",
                r#"{"uid":502,"name":"opie","generatedUid":"6DB4B7AF-BE3C-486C-BD46-1B4558946919"}"#,
                "--logged-out",
            ]
        );
    }

    #[test]
    fn an_account_that_changes_after_it_was_checked_is_never_readied() {
        // Its user ID moves while root checks whether it is standard.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = vec![
            ("/usr/bin/id", OPIE),
            ("/usr/sbin/dseditgroup", "touch moved; exit 67"),
        ];
        tools.extend(STANDARD.into_iter().skip(1));
        let text = run_steps(
            dir.path(),
            &tools,
            &["choose", "opie", EXECUTABLE, "502", GUID],
        );
        assert_eq!(
            outcome(&text),
            Err(Refusal::Failed(
                "opie does not have the user ID Atlas checked".into()
            ))
        );
        assert!(!asked(dir.path(), "createhomedir"));

        // Another account than the one checked, with the same name and ID.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("reused"), "").unwrap();
        let mut tools = vec![("/usr/bin/id", OPIE)];
        tools.extend(STANDARD);
        let text = run_steps(
            dir.path(),
            &tools,
            &["choose", "opie", EXECUTABLE, "502", GUID],
        );
        assert_eq!(
            outcome(&text),
            Err(Refusal::Failed(
                "opie is not the account Atlas checked".into()
            ))
        );
        assert!(!asked(dir.path(), "createhomedir"));

        // Replaced once its home was made: it is not readied as the helper.
        let dir = tempfile::tempdir().unwrap();
        let mut tools = vec![
            ("/usr/bin/id", OPIE),
            (
                "/usr/sbin/createhomedir",
                r#"print -r -- "createhomedir $*" >> calls; touch reused"#,
            ),
        ];
        tools.extend(
            STANDARD
                .into_iter()
                .filter(|(tool, _)| *tool != "/usr/sbin/createhomedir"),
        );
        let text = run_steps(
            dir.path(),
            &tools,
            &["choose", "opie", EXECUTABLE, "502", GUID],
        );
        assert_eq!(
            outcome(&text),
            Err(Refusal::Failed(
                "opie is not the account Atlas checked".into()
            ))
        );
        assert!(!dir.path().join("sudo.log").exists());
    }

    #[test]
    fn an_account_whose_user_id_changed_since_it_was_checked_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = vec![("/usr/bin/id", OPIE)];
        tools.extend(STANDARD);
        let text = run_steps(
            dir.path(),
            &tools,
            &["choose", "opie", EXECUTABLE, "503", GUID],
        );

        assert_eq!(
            outcome(&text),
            Err(Refusal::Failed(
                "opie does not have the user ID Atlas checked".into()
            ))
        );
        assert!(!dir.path().join("sudo.log").exists());
    }

    #[test]
    fn a_creation_that_fails_silently_reports_sysadminctls_real_status() {
        let dir = tempfile::tempdir().unwrap();
        let mut script = ["stage_executable() { :; }\n", STEPS].concat();
        let tools = [
            ("/usr/bin/id", "exit 1"),
            ("/usr/bin/dscl", "exit 0"),
            ("/usr/bin/pgrep", "exit 1"),
            ("/usr/sbin/sysadminctl", "exit 3"),
        ];
        for (tool, body) in tools {
            let path = dir.path().join(Path::new(tool).file_name().unwrap());
            fs::write(&path, format!("#!/bin/zsh -f\n{body}\n")).unwrap();
            fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
            script = script.replace(tool, &path.to_string_lossy());
        }

        let (ours, theirs) = UnixStream::pair().unwrap();
        let mut child = Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args(["create", "opie", EXECUTABLE, "502", "Opie", ""])
            .stdin(std::os::fd::OwnedFd::from(theirs.try_clone().unwrap()))
            .stdout(std::os::fd::OwnedFd::from(theirs))
            .spawn()
            .unwrap();
        let (_, text) = converse(ours, Some(b"secret"));
        child.wait().unwrap();

        assert_eq!(
            outcome(&text.unwrap()),
            Err(Refusal::Failed(
                "macOS did not create opie (sysadminctl stopped with status 3)".into()
            ))
        );
    }

    #[test]
    fn creating_types_the_password_into_sysadminctl_over_the_approved_steps_socket() {
        let dir = tempfile::tempdir().unwrap();
        // The real script, on a socket as the approval gives it; sysadminctl
        // stands in, asking as it does and noting what it got.
        let mut tools = vec![
            (
                "/usr/bin/id",
                r#"[[ -e made ]] || exit 1; [[ $1 == -nu ]] && print opie || print 502"#,
            ),
            (
                "/usr/sbin/sysadminctl",
                r#"print -rl -- "$@" > sysadminctl.log; cp "${@[-1]}" picture.png
read -rs "password?User password:"; print -rn -- $password > password; touch made"#,
            ),
        ];
        tools.extend(STANDARD);
        let mut script = ["stage_executable() { :; }\n", STEPS].concat();
        for (tool, body) in &tools {
            let path = dir.path().join(Path::new(tool).file_name().unwrap());
            fs::write(
                &path,
                format!("#!/bin/zsh -f\ncd {}\n{body}\n", dir.path().display()),
            )
            .unwrap();
            fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
                .unwrap();
            script = script.replace(tool, &path.to_string_lossy());
        }
        let picture = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(PICTURE)
        };
        let full = "Atlas $x * helper";

        let (ours, theirs) = UnixStream::pair().unwrap();
        let mut child = Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args(["create", "opie", EXECUTABLE, "502", full, &picture])
            .stdin(std::os::fd::OwnedFd::from(theirs.try_clone().unwrap()))
            .stdout(std::os::fd::OwnedFd::from(theirs))
            .spawn()
            .unwrap();
        let (finished, text) = converse(ours, Some(b"se cret$1"));
        child.wait().unwrap();
        let text = text.unwrap();

        assert!(finished);
        assert_eq!(outcome(&text), Ok(()), "{text}");
        assert!(text.contains("@atlas created 0"), "{text}");
        assert!(!text.contains("se cret$1"), "the password never comes back");
        assert_eq!(
            fs::read_to_string(dir.path().join("password")).unwrap(),
            "se cret$1"
        );
        let logged = fs::read_to_string(dir.path().join("sysadminctl.log")).unwrap();
        let arguments: Vec<&str> = logged.lines().collect();
        assert_eq!(
            arguments[..8],
            [
                "-addUser",
                "opie",
                "-fullName",
                full,
                "-UID",
                "502",
                "-password",
                "-"
            ]
        );
        assert_eq!(arguments[8], "-picture");
        assert_eq!(fs::read(dir.path().join("picture.png")).unwrap(), PICTURE);
    }
}
