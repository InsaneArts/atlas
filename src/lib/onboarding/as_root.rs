//! The step that runs as root under the person's one approval: creating the
//! helper account and readying it for its first login. Only Apple's own
//! tools run as root, from a script fixed here, so no file the person can
//! change runs as root or is read by it; Atlas itself runs only as the
//! helper, readying that account (`setup-helper`).
use super::{APPROVAL, MAX_ANSWERS, answer, last_words, signed_executable, without};
use crate::{
    approval::{self, Refusal},
    init,
};
use serde_json::Value;
use std::{os::unix::net::UnixStream, path::Path, time::Instant};

/// What each line the script reports a step's outcome with starts with.
const MARK: &str = "@atlas ";

/// Root for good, with root's own environment: zsh sets every user ID on
/// `UID=0` when it may, and a refusal would end zsh itself, so it is tried
/// in a subshell first. Standard error goes nowhere (it is the asking
/// Atlas's own), and nothing of the asking process carries over that could
/// shape what root makes: its file mode mask or its limits.
const BECOME_ROOT: &str = r##"if ! ( UID=0 ) 2>/dev/null; then print -r -- "@atlas error macOS did not let Atlas act as an administrator"; exit 1; fi
UID=0
GID=0
exec 2>/dev/null
umask 022
ulimit -f unlimited
export HOME=/var/root LOGNAME=root USER=root
unset OLDPWD
cd /
"##;

/// The steps, as root. Arguments: `create NAME EXECUTABLE UID FULL-NAME
/// PICTURE` (the picture in base64) or `choose NAME EXECUTABLE UID`, where
/// UID is the one the asking Atlas chose or checked. Each outcome is a line
/// starting with [`MARK`].
const STEPS: &str = r##"refuse() { print -r -- "@atlas error $*"; exit 1 }
mode=$1 name=$2 executable=$3 expected=$4
# Checked again here, as root: an account name (never only digits, which id
# reads as a user ID), and only a person's account: macOS gives the user IDs
# below 501 to itself.
[[ $name =~ '^[A-Za-z0-9_][A-Za-z0-9._-]*$' && $name != <-> ]] || refuse "$name is not an account name Atlas uses"
[[ $expected == <501-60000> ]] || refuse "$name is not a person's account"
uid=$(/usr/bin/id -u -- $name)
if [[ $mode == create ]]; then
  [[ -z $uid ]] || refuse "$name already exists"
  [[ -z $(/usr/bin/dscl . -search /Users UniqueID $expected </dev/null) ]] || refuse "user ID $expected is taken now; try again"
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
# dseditgroup answers 67 for an account outside the group: a standard one.
/usr/sbin/dseditgroup -o checkmember -m $name admin >/dev/null </dev/null
[[ $? == 67 ]] || refuse "$name is an administrator, or macOS could not tell"
/usr/sbin/createhomedir -c -u $name >/dev/null </dev/null
request="{\"action\":\"first-login\",\"uid\":$uid,\"opener\":true}"
# Its answer is on standard output, a refusal on standard error: kept apart,
# so a stray warning never spoils the answer.
errors=$(/usr/bin/mktemp)
# Through the account's login when it has one, so launchctl reaches it there.
if /usr/bin/pgrep -qxu $uid loginwindow; then
  readied=$(/bin/launchctl asuser $uid /usr/bin/sudo -n -u $name $executable setup-helper $request 2>$errors </dev/null)
else
  readied=$(/usr/bin/sudo -n -u $name $executable setup-helper $request 2>$errors </dev/null)
fi
[[ -n $readied ]] || readied=$(<$errors)
/bin/rm -f -- $errors
print -r -- "@atlas readied $readied"
"##;

/// The helper account's picture, Atlas's mark, carried in Atlas's signed
/// executable rather than read from the app's folder.
const PICTURE: &[u8] = include_bytes!("../../../app/Resources/HelperPicture.png");

/// Create the account `name` with user ID `uid`, called `full_name`, typing
/// `password` into macOS's question, and ready it for its first login.
pub(super) fn create(
    name: &str,
    uid: u32,
    full_name: &str,
    password: &[u8],
) -> Result<(), Refusal> {
    let picture = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(PICTURE)
    };
    run(
        &format!("Atlas will create “{name}” and get it ready to be its helper."),
        &["create", name],
        uid,
        &[full_name, &picture],
        Some(password),
    )
}

/// Ready the existing account `name` (user ID `uid`) for its next login.
pub(super) fn ready(name: &str, uid: u32) -> Result<(), Refusal> {
    run(
        &format!("Atlas will get “{name}” ready to be its helper."),
        &["choose", name],
        uid,
        &[],
        None,
    )
}

/// Run the steps for `mode` and `name` under one approval, `prompt` shown in
/// macOS's dialog, with the account's user ID `uid` and the `rest` of the
/// arguments, answering any password question with `secret`.
fn run(
    prompt: &str,
    [mode, name]: &[&str; 2],
    uid: u32,
    rest: &[&str],
    secret: Option<&[u8]>,
) -> Result<(), Refusal> {
    let executable = signed_executable().map_err(|error| Refusal::Failed(error.error))?;
    let executable = executable.to_string_lossy();
    let script = [BECOME_ROOT, STEPS].concat();
    let uid = uid.to_string();
    let mut argv = vec![
        "-i",
        "PATH=/usr/bin:/bin:/usr/sbin:/sbin",
        "/bin/zsh",
        "-p",
        "-f",
        "-c",
        &script,
        "zsh",
        mode,
        name,
        &executable,
        &uid,
    ];
    argv.extend_from_slice(rest);

    let stream = approval::run(Path::new("/usr/bin/env"), &argv, prompt)?;
    let (finished, text) = converse(stream, secret);
    reap(finished);
    outcome(&text?)
}

/// Talk with the approved step over its channel until it ends: each
/// password question answered with `secret`, and everything kept. Whether
/// it ended (rather than Atlas giving up on it), and what it said. The step
/// never writes the secret back; only what sysadminctl wrote before the
/// first step's line could hold it, and it is cut out there.
fn converse(mut stream: UnixStream, secret: Option<&[u8]>) -> (bool, Result<String, Refusal>) {
    let (secret, answers) = secret.map_or((&[][..], 0), |secret| (secret, MAX_ANSWERS));
    let channel = match stream.try_clone() {
        Ok(channel) => channel,
        Err(error) => return (false, Err(Refusal::Failed(error.to_string()))),
    };
    let receiver = init::forward(channel);
    let mut seen = Vec::new();
    let answered = answer(
        &mut stream,
        &receiver,
        secret,
        answers,
        Instant::now() + APPROVAL,
        &mut seen,
    );
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

/// Reap the approved step, this process's child: waiting for it once it
/// has ended its output (`finished`), else only looking for a moment, so a
/// step stuck past its time never holds this process.
fn reap(finished: bool) {
    use nix::sys::wait::{WaitPidFlag, WaitStatus, wait, waitpid};

    if finished {
        let _ = wait();
        return;
    }

    for _ in 0..10 {
        match waitpid(None, Some(WaitPidFlag::WNOHANG)) {
            Ok(WaitStatus::StillAlive) => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            _ => return,
        }
    }
}

/// What the steps' lines say: a refusal (when a creation made no account,
/// sysadminctl's own last words say why), or the readying's reply, which
/// must be ok. The reply is `setup-helper`'s JSON over several lines, so it
/// is the rest of the text after its mark.
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
    use super::*;
    use crate::setup::helper::Request;
    use std::{fs, process::Command};

    /// The steps run as this account, Apple's tools standing in as scripts
    /// in `dir` that `tools` names; the stand-ins share `dir` for notes.
    fn run_steps(dir: &Path, tools: &[(&str, &str)], arguments: &[&str]) -> String {
        let mut script = STEPS.to_owned();
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

        let run = Command::new("/usr/bin/env")
            .args(["-i", "/bin/zsh", "-p", "-f", "-c", &script, "zsh"])
            .args(arguments)
            .output()
            .unwrap();
        String::from_utf8_lossy(&run.stdout).into_owned()
    }

    const STANDARD: [(&str, &str); 4] = [
        ("/usr/sbin/dseditgroup", "exit 67"),
        ("/usr/sbin/createhomedir", "exit 0"),
        ("/usr/bin/pgrep", "exit 1"),
        (
            "/usr/bin/sudo",
            r#"print -rl -- "$@" > sudo.log; print -r -- '{"ok": true}'"#,
        ),
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
        let script = [BECOME_ROOT, STEPS].concat();
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
    fn readying_hands_names_and_paths_to_each_tool_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = vec![("/usr/bin/id", "print 502")];
        tools.extend(STANDARD);
        let text = run_steps(dir.path(), &tools, &["choose", "opie", EXECUTABLE, "502"]);

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
            ]
        );
    }

    #[test]
    fn an_account_whose_user_id_changed_since_it_was_checked_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let mut tools = vec![("/usr/bin/id", "print 502")];
        tools.extend(STANDARD);
        let text = run_steps(dir.path(), &tools, &["choose", "opie", EXECUTABLE, "503"]);

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
        let mut script = STEPS.to_owned();
        let tools = [
            ("/usr/bin/id", "exit 1"),
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
            ("/usr/bin/id", "[[ -e made ]] && print 502"),
            (
                "/usr/sbin/sysadminctl",
                r#"print -rl -- "$@" > sysadminctl.log; cp "${@[-1]}" picture.png
read -rs "password?User password:"; print -rn -- $password > password; touch made"#,
            ),
        ];
        tools.extend(STANDARD);
        let mut script = STEPS.to_owned();
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
