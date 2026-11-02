use atlas_core::error::{Code, Error, Result as AtlasResult};
use atlas_native::approval::Refusal;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// What each line the script reports a step's outcome with starts with.
pub const MARK: &str = "@atlas ";

/// The search path privileged scripts run with, as `env` takes it.
pub const ROOT_PATH: &str = "PATH=/usr/bin:/bin:/usr/sbin:/sbin";

/// Identify the approved executor before it can act. If its caller cannot
/// read the identity or goes away before acknowledging it, the script exits
/// without reaching any privileged work.
pub const EXECUTOR: &str = r##"print -r -- "@atlas executor $$"
IFS= read -r acknowledgement || exit 1
[[ $acknowledgement == "@atlas continue $$" ]] || exit 1
"##;

/// Run a privileged command with the durable executor handshake. Its script
/// starts with [`EXECUTOR`]. Interactive tools use the controlling tty; stdin
/// carries only the acknowledgement sent after all bound claims are synced.
///
/// # Errors
///
/// Returns a [`Refusal`] when the executor can't start or be waited for.
pub fn tracked_output(command: &mut Command) -> Result<std::process::Output, Refusal> {
    let (child, pid) = start_executor(command)?;
    let output = child
        .wait_with_output()
        .map_err(|error| Refusal::Failed(error.to_string()))?;
    wait_until_ended(pid);
    Ok(output)
}

/// Start the fixed executor and durably identify it before permitting work.
///
///
/// # Errors
///
/// Returns a [`Refusal`] when the executor can't start or be identified.
///
/// # Panics
///
/// Never: spawn creates both requested pipes.
#[allow(clippy::expect_used, reason = "spawn creates both requested pipes")]
pub fn start_executor(
    command: &mut Command,
) -> Result<(std::process::Child, nix::unistd::Pid), Refusal> {
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
pub fn wait_until_ended(pid: nix::unistd::Pid) {
    while nix::sys::signal::kill(pid, None) != Err(nix::errno::Errno::ESRCH) {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Keep the caller, its ownership and its claims alive until this exact
/// executor exits, even if its reply times out or its output closes early.
pub fn finish_executor(
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
pub(crate) fn converse_until(
    mut stream: UnixStream,
    secret: Option<&[u8]>,
    deadline: Instant,
) -> (bool, Result<String, Refusal>) {
    let (secret, answers) = secret.map_or((&[][..], 0), |secret| (secret, MAX_ANSWERS));
    let channel = match stream.try_clone() {
        Ok(channel) => channel,
        Err(error) => return (false, Err(Refusal::Failed(error.to_string()))),
    };
    let receiver = forward(channel);
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
fn executor(stream: &mut impl Read) -> Result<nix::unistd::Pid, Refusal> {
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

/// How long a step under the person's approval may take, the approval
/// included.
pub const APPROVAL: Duration = Duration::from_secs(10 * 60);

pub fn problem(message: impl Into<String>) -> Error {
    Error::new(Code::Onboarding, message)
}

/// A password, zeroed when dropped.
pub struct Secret(pub Vec<u8>);

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.fill(0);
    }
}

/// The last thing a program said: its last line that is not empty and not a
/// password question.
pub fn last_words(text: &str) -> Option<&str> {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty() && !line.to_ascii_lowercase().ends_with("password:"))
}

/// A password from standard input: one line of 1 to 256 bytes, with no
/// control characters (a terminal would edit the line with them, so macOS
/// would receive a different password than the one typed).
///
/// # Errors
///
/// Returns an error for input that is not one such line.
pub fn read_secret(input: impl Read) -> AtlasResult<Vec<u8>> {
    // To the end: the window closes its input after the password.
    let mut secret = atlas_core::files::read_up_to(input, 258)?;

    if secret.last() == Some(&b'\n') {
        secret.pop();
    }

    if secret.is_empty()
        || secret.len() > 256
        || secret.iter().any(|byte| *byte < 0x20 || *byte == 0x7F)
    {
        secret.fill(0);
        return Err(problem(
            "Give one password of 1 to 256 characters, without control characters",
        ));
    }

    Ok(secret)
}

/// Bind the terminal worker before it executes `program`, then type `secret`
/// only when asked. Its exact PID stays in the transaction's live claim even
/// if the caller dies; the secret is cut out of anything kept from output.
///
/// # Errors
///
/// Returns the failure to start the worker, type `secret` or wait for it.
pub fn answer_prompts(
    program: &str,
    args: &[&str],
    secret: &[u8],
    patience: Duration,
) -> AtlasResult<String> {
    let script = [EXECUTOR, "exec \"$@\"\n"].concat();
    let (mut child, executor) = start_executor(
        Command::new("/usr/bin/script")
            .args([
                "-q",
                "/dev/null",
                "/bin/zsh",
                "-f",
                "-c",
                &script,
                "zsh",
                program,
            ])
            .args(args)
            .stderr(Stdio::piped()),
    )
    .map_err(|refusal| {
        problem(format!("The terminal worker did not start: {refusal:?}")).uncertain()
    })?;

    let mut input = child.stdin.take().ok_or_else(|| problem("No input pipe"))?;
    let output = child
        .stdout
        .take()
        .ok_or_else(|| problem("No output pipe"))?;
    let errors = child
        .stderr
        .take()
        .ok_or_else(|| problem("No error pipe"))?;

    let receiver = forward(output);
    // Drained as it comes, so a chatty program never blocks on it.
    let complaints = forward(errors);
    let mut seen = Vec::new();
    let answered = answer(
        &mut input,
        &receiver,
        secret,
        MAX_ANSWERS,
        Instant::now() + patience,
        &mut seen,
    );
    if let Err(error) = answered {
        let _ = child.kill();
        let _ = child.wait();
        wait_until_ended(executor);
        seen.fill(0);
        return Err(error);
    }

    drop(input);
    let status = child.wait()?;
    wait_until_ended(executor);

    // The secret is cut out byte for byte, never copied into a string.
    let kept = without(&seen, secret);
    seen.fill(0);
    let text = String::from_utf8_lossy(&kept).into_owned();

    if status.success() {
        return Ok(text);
    }

    let complained: Vec<u8> = complaints.try_iter().flatten().collect();
    let complained = String::from_utf8_lossy(&without(&complained, secret)).into_owned();
    let summary = last_words(&text)
        .or_else(|| last_words(&complained))
        .unwrap_or_default()
        .to_owned();

    Err(problem(summary))
}

/// Read what a program writes until it stops (its output closes) or
/// `deadline` passes, collecting it into `seen`, and type `secret` and a
/// newline into `input` each time it has asked for a password, up to
/// `answers` times.
fn answer(
    input: &mut impl Write,
    receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
    secret: &[u8],
    answers: usize,
    deadline: Instant,
    seen: &mut Vec<u8>,
) -> AtlasResult<()> {
    let mut answered = 0;

    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(bytes) => seen.extend(bytes),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Ok(()),
        }

        // A question ends without a newline: "User password:".
        let asked = seen
            .windows(PROMPT.len())
            .filter(|window| window.eq_ignore_ascii_case(PROMPT))
            .count();
        if asked > answered && answered < answers {
            // Let the prompt turn echo off before typing.
            std::thread::sleep(Duration::from_millis(150));

            input
                .write_all(secret)
                .and_then(|()| input.write_all(b"\n"))
                .and_then(|()| input.flush())?;
            answered += 1;
        }

        if Instant::now() >= deadline {
            return Err(problem("The step took too long and was stopped"));
        }
    }
}

/// What a password question ends with, in any case.
const PROMPT: &[u8] = b"password:";

/// The most password questions answered, so a program that keeps asking
/// (after a refused password, say) is not fed the secret forever.
const MAX_ANSWERS: usize = 3;

/// `bytes` with every occurrence of `secret` cut out.
pub fn without(bytes: &[u8], secret: &[u8]) -> Vec<u8> {
    if secret.is_empty() {
        return bytes.to_vec();
    }

    let mut kept = Vec::with_capacity(bytes.len());
    let mut rest = bytes;
    while !rest.is_empty() {
        if rest.starts_with(secret) {
            rest = &rest[secret.len()..];
        } else {
            kept.push(rest[0]);
            rest = &rest[1..];
        }
    }

    kept
}

/// Read `stream` on a thread of its own, passing each chunk on as it
/// arrives; the channel closes when the stream ends.
pub fn forward(mut stream: impl Read + Send + 'static) -> std::sync::mpsc::Receiver<Vec<u8>> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 4096];
        while let Ok(read @ 1..) = stream.read(&mut chunk) {
            if sender.send(chunk[..read].to_vec()).is_err() {
                break;
            }
        }
    });

    receiver
}

/// Like `attended`, but timestamped log lines on standard error (as
/// sysadminctl writes) are dropped; anything else, such as a password
/// question, is shown as it arrives. On failure the dropped lines are shown.
///
/// # Errors
///
/// Returns an error when the command fails or can't be watched.
pub fn quietly_attended(mut child: std::process::Child, why: &str) -> AtlasResult<()> {
    let output = child.stdout.take().map(|mut stream| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).map(|_| bytes)
        })
    });

    let stream = child
        .stderr
        .take()
        .ok_or_else(|| super::account::problem("The command has no error output"))?;
    let receiver = forward(stream);

    let log_line = |bytes: &[u8]| {
        // "2026-10-11 14:28:19.767 sysadminctl[…]": a date, then a time.
        bytes.len() > 20
            && bytes[..4].iter().all(u8::is_ascii_digit)
            && bytes[4] == b'-'
            && bytes[10] == b' '
            && bytes[13] == b':'
    };

    let mut dropped = Vec::new();
    let mut pending = Vec::new();
    let mut terminal = std::io::stderr();

    loop {
        match receiver.recv_timeout(Duration::from_millis(250)) {
            Ok(bytes) => {
                pending.extend(bytes);

                while let Some(end) = pending.iter().position(|&byte| byte == b'\n') {
                    let line: Vec<u8> = pending.drain(..=end).collect();
                    if log_line(&line) {
                        dropped.extend(line);
                    } else {
                        let _ = terminal.write_all(&line);
                    }
                }
            }
            // A question waits for an answer without ending its line.
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if !pending.is_empty() && !log_line(&pending) {
                    let _ = terminal.write_all(&pending);
                    let _ = terminal.flush();
                    pending.clear();
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    let status = child.wait()?;
    let output = output
        .and_then(|reader| reader.join().ok())
        .transpose()?
        .unwrap_or_default();
    if status.success() {
        return Ok(());
    }

    let _ = terminal.write_all(&dropped);
    let _ = terminal.write_all(&pending);
    let _ = terminal.write_all(&output);

    Err(super::account::problem(why.to_owned()))
}

/// Hold and claim synchronous setup before any worker can change the helper.
/// Only a complete acknowledgement settles this transaction's generation.
///
/// # Errors
///
/// Returns the failure to take control or claim the work, or what the work
/// returns.
pub fn setup_transaction<T>(
    controller: &crate::control::Controller,
    act: impl FnOnce() -> AtlasResult<T>,
) -> AtlasResult<T> {
    let _ownership = controller.acquire(true)?;
    setup_transaction_held(controller, act)
}

/// # Errors
///
/// Returns the failure to claim the work, or what the work returns.
pub fn setup_transaction_held<T>(
    controller: &crate::control::Controller,
    act: impl FnOnce() -> AtlasResult<T>,
) -> AtlasResult<T> {
    let pending = controller.quarantine()?;
    let _executors = crate::control::ExecutorClaims::new(std::slice::from_ref(&pending))?;
    let outcome = act();
    if outcome.is_ok() {
        controller.clear_if(&pending)?;
    }
    outcome
}

/// Root for good, with root's own environment: zsh sets every user ID on
/// `UID=0` when it may, and a refusal would end zsh itself, so it is tried
/// in a subshell first. The account asking is noted first: sudo names it,
/// and macOS's approval keeps it as the real user ID. Standard error goes
/// nowhere (it is the asking Atlas's own), and nothing of the asking process
/// carries over that could shape what root makes: its file mode mask or its
/// limits.
pub const BECOME_ROOT: &str = r##"asker=${SUDO_UID:-$UID}
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

pub fn staging() -> String {
    STAGING
        .replace(
            "ATLAS_WORKER_REQUIREMENT",
            &atlas_core::app_bundle::requirement_for(atlas_core::app_bundle::IDENTIFIER),
        )
        .replace(
            "ATLAS_APP_REQUIREMENT",
            &atlas_core::app_bundle::requirement_for(atlas_core::app_bundle::APP_IDENTIFIER),
        )
}

/// Request normal sudo consent and refresh it while attended setup waits.
/// Returns an init error when sudo refuses administrator rights.
///
/// # Errors
///
/// Returns missing-terminal, cancelled-authorization or administrator-rights failures.
/// Authorization uses the operating system's normal consent flow.
pub fn request_administrator_rights() -> AtlasResult<()> {
    let status = Command::new("/usr/bin/sudo").arg("-v").status()?;
    if !status.success() {
        return Err(super::account::problem(
            "Administrator rights were not granted",
        ));
    }

    // sudo forgets the rights after five minutes while init waits for the person.
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(60));
            let _ = Command::new("/usr/bin/sudo").args(["-n", "-v"]).output();
        }
    });
    Ok(())
}

/// Check cached rights without prompting after a long first-login wait.
pub fn administrator_rights_current() -> bool {
    Command::new("/usr/bin/sudo")
        .args(["-n", "-v"])
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests;
