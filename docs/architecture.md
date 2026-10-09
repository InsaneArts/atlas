# Architecture

Understudy is a small Python client and a local browser interface around the
frameworks. The installed helper remains in its consented location.

| Module | Responsibility |
| --- | --- |
| `config.py` | Validated configuration and paths relative to its file |
| `transport.py` | Bounded JSONL exchange, total deadline, protocol errors |
| `session.py` | Hidden account identity, display geometry and readiness |
| `actions.py` | Complete input validation and duration limits |
| `artifacts.py` | Private explicit screenshots and rotated metadata journals |
| `control.py` | Cooperative process lock, persistent uncertainty and per-tab leases |
| `client.py` | Compose policy, transport, ownership and artifacts |
| `monitor.py` | Local HTTP routes, capability/origin checks and frame cache |
| `web/` | Packaged HTML, CSS, input mapping and a single-flight preview loop |
| `cli.py` | Arguments and JSON results |

## Observation

A frame comes from the actual helper screenshot operation. Identity is checked
before and after capture. The viewer retains a bounded in-memory frame cache;
PNG encoding does not hold the input metadata lock. Each frame carries its
helper UID, daemon PID and display geometry. Input must reference a recent frame
whose identity still matches. Coordinates map through the actual contained image
rectangle to full display points, including letterboxing. Retina mapping has unit
coverage; a real Retina desktop remains unqualified.

The preview has one active poll, cancellation and generation checks. Pausing or
hiding the page cannot let a late response restart polling or replace its state.
Viewing defaults to two-second intervals; control requests fresh frames more
frequently. This is a screenshot viewer, not a video stream.

## Control

Take control acquires a per-tab lease and a process lock keyed by the canonical
socket path and UID. CLI input, run and restart use the same lock. A second
clients can bypass this cooperative mechanism; native service-enforced ownership
is still missing.

The browser sends complete clicks, keys, text chunks and drags. A drag is sent
when the pointer is released. There are no held mouse/key primitives. Pointer
movement is throttled; wheel events are coalesced. Printable Unicode comes from
committed browser text, including composition. An explicit browser paste sends
local text; shortcut buttons use the helper's own clipboard. There is no
clipboard synchronization or background clipboard reading.

A lease lasts ten seconds and is renewed by a visible, focused viewer. Pause,
blur, tab hiding, stale pixels, session changes and explicit release discard
unsent browser input. The server expires abandoned leases independently of HTTP
traffic. Release during identity preflight cancels dispatch; a dispatched native
action may finish. Its lock remains held until the response settles. The stock
native helper still lacks atomic mid-action transition cancellation.

All input batches are validated before native dispatch: finite in-display
coordinates, supported keys/buttons, integral bounded counts/scroll/waits,
1–32 actions and at most a two-second estimated action budget. Text actions are
limited to 256 code points; the browser sends smaller chunks and caps a paste at
2,048 code points. Unsupported or oversized input is refused in full.

Before mutation, a pending marker is persisted. It survives controller crashes
or interruption. A complete response clears it; unknown delivery blocks further
control, without replay. Observe the desktop and run `understudy restart` to
recover. Restart retains ownership until a new hidden helper PID is confirmed.
An unsuccessful restart does not clear prior uncertainty.

## HTTP and storage

The viewer binds only to `127.0.0.1`. Its random capability URL is stored in a
private local connection file. Input also requires a valid per-tab lease,
exact same-origin POST, JSON body and a recent frame. There is no browser endpoint
for launching commands or restarting the helper. Request bodies, transport
responses/events, frame retention and operation durations are bounded.

Routine metadata contains operation names, timings, result codes and counts.
It excludes typed text, command arguments/output, window titles and image data.
The journal rotates at one MiB, keeping one previous file. Explicit CLI captures
are private files. Logging failure preserves an acknowledged operation's result.
Original investigation logs are preserved separately and remain Git-ignored.

## Audit disposition

Two agents independently designed the structure and audited the original code.
The rewrite addresses the concrete findings: unsafe numeric conversion exposure,
malformed replies and missing PNGs, unbounded protocol reads, payload logging,
logging failures after successful input, configuration crashes, incomplete
controller locking, interrupted-action ambiguity, overlapping preview requests,
static asset packaging and ignored curated evidence. Regression tests cover
these boundaries. The native helper and historical `live/` source were unchanged.

Measured native limitations are retained in [live-results.md](live-results.md).
They are not reclassified as fixed by wrapper changes.
