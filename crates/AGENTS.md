# Rust work in Atlas

Read the repository's `AGENTS.md` first. Paths here are relative to `crates/`.
Each folder is one layer and one package, and each package depends only on the
layers above it in this list:

| Folder   | Package        | What it holds                                                      |
| -------- | -------------- | ------------------------------------------------------------------ |
| `core`   | `atlas-core`   | names, errors, time, wire types, observations, install location    |
| `native` | `atlas-native` | macOS calls and input execution                                    |
| `daemon` | `atlas-daemon` | the helper daemon, and the helper's layout and launchd actions     |
| `client` | `atlas-client` | the controller side: verified requests, control, setup, agents     |
| `cli`    | `atlas-cli`    | the `atlas` binary, viewer, MCP server, menu bar, onboarding, `ui` |

Command grammar and attended interaction stay in `cli/src/bin/atlas`, release
tooling in `xtask`, which sits above every crate.

## Crate boundaries

Cargo enforces the layers: a crate cannot name one below it in that table, in
production or in tests. Put a new module in the lowest crate that can hold it.
Add a new crate only when a layer needs one, and keep the dependency direction.

An item crosses a boundary with `pub` only when a real caller needs it; everything
else stays `pub(crate)`. Callers import from the owning crate. There is no facade
crate and no re-export of another crate's items. An inherent or trait impl that
the orphan rule refuses moves to the type's owner; do not add a wrapper or trait.

Shared dependencies and lints live in the root `Cargo.toml`. Each crate lists only
the dependencies and features it uses, including `[dev-dependencies]`.

`core`, `native`, `daemon` and `client` have a `test-support` feature for what the
tests of crates above need, enabled only through `[dev-dependencies]`. Gate such
items with `#[cfg(any(test, feature = "test-support"))]`, and any `cfg(test)`
switch that a test above relies on. A test that reaches a higher layer lives in
that layer's crate. Shipped builds never carry the feature. `cargo test` does build
`target/debug/atlas` with it, so run `cargo build` before using that binary.

## Owners and boundaries

- `agent` resolves targets and adapts agent operations; `client` verifies the
  helper, transports requests, and owns controller-side mutation lifetimes.
- `control` owns exclusion, leases and pending mutations. `host_session` owns
  attribution and session records; `task` owns the work attributed to a task.
- `daemon` serves capture, reads, commands and jobs. `native` owns macOS calls,
  input execution, cancellation, grants and held-input recovery.
- `screen` owns portable observed geometry, windows, elements and selection.
  Interpreting an observation must not introduce a native effect.
- `environment` owns staging, application and rollback. Reuse its existing
  concrete transaction and record owners rather than adding a persistence layer.
- `helper_setup` owns the helper's layout, launchd actions, first login and
  pairing, runs inside the helper and never reaches `client`. `setup` owns the
  controller's side: the executor, configuration and migration. `onboarding`
  orchestrates their workflow. Terminal prompts belong in `cli/src/bin/atlas`.
- `viewer` owns its HTTP service and personal-control lifetime; `mcp` adapts its
  declared tools. `ui` formats output; domain authority must not depend on it.
- Shared names, errors and time belong in their existing leaf modules; wire
  types in `protocol`, host identity in `host`, accounts in `account`, where
  Atlas is installed and who signed it in `app_bundle`. A leaf never reaches up
  to `client`, `setup` or a domain owner. Keep observation, wire projections
  and durable recovery models distinct.

Use the narrowest visibility that real binary, tooling or supported library
callers require. Tests alone do not justify a shipping forwarding adapter.
Keep a complete operation together. A large exhaustive router can be clearer
than several routers that silently omit cases. Extract only a named operation,
a resource lifetime, an invariant or genuinely shared logic. Do not add a
generic command bus, handler framework, registry or repository to shorten files.

## Contracts and proof

Preserve words, exit status, JSON field presence, ordering, duplicate-member
semantics, stored formats, refusal precedence and timeout guarantees unless the
task explicitly authorizes a behavior change. Decode at ingress and keep typed
facts through domain decisions. Retain original values only for presentation or
intentional extension fields. Validate an entire batch before its first effect.

An optional field must deliberately distinguish missing, null, wrong type and
invalid value. Unknown members and duplicate members need the boundary's actual
compatibility policy. Moving validation earlier can change which refusal wins.
Enums express alternatives; boolean observation facts may remain independent.
Document retained state combinations and validate durable state when loading it.

Retain actionable error causes internally and expose only reviewed public facts.
Public fallible operations document errors, side effects and uncertainty; use
helper-free examples for pure validation. Every ignored result needs a concrete
best-effort, receiver-gone or retained-recovery reason. Prove every index locally
or use checked access. Do not replace useful test assertions with silent defaults.

Keep UID, graphical-session, off-console and exact display-epoch guards. Input
goes only to the session event tap. Normal macOS consent and the same Developer
ID identity remain mandatory. Never modify TCC or use a global HID fallback.
Unsafe code stays denied; prefer safe bindings, and give every necessary local
allowance its reason and every unsafe block its `SAFETY` explanation.

Cancellation must not wait for the execution lock. Register work before effects,
retain its owner through cleanup and join registered blocking work. Lost replies
and timeouts never acknowledge mutations. Preserve unknown records until verified
recovery. Describe nested lock order and intentional guards across awaits next
to the owner. Isolate a complete slow operation from runtime workers.

Tests name outcomes as sentences. Use barriers, readiness messages and controlled
time rather than sleeps to order ordinary tests. Real-child tests may use an
outer watchdog. Unit tests and custom views are not native-helper evidence.
Coordinate helper ownership before live work and physical lock, sleep or user
switches with the person. Check installed hashes and the running service's identity.

Logs need an owner, private permissions, a size/retention rule and a failure
policy. Never log typed text, private tokens or arbitrary request/error payloads.
Use producer canaries and explicit projections; a safe shared error type cannot
prove all callers are safe. Dependency features need an actual source, test or
target use. Do not change the compiler minimum or add dependencies to bypass a check.

## Adding a feature

### CLI helper verb

Edit `cli/src/bin/atlas/cli.rs`, the complete row in `cli/src/bin/atlas/policy.rs::Policy::of`, the dispatch
arm in `cli/src/bin/atlas/dispatch.rs` and its group in `cli/src/bin/atlas/help.rs`. Call the existing domain
operation. The policy and dispatch matches are exhaustive; the help grouping and
activity wording still need review. Test human and JSON results, Stop admission,
waiting, interruption and task attribution.

### Read-only observation

Add the CLI decisions above and its read operation. Reuse verified, bounded,
certain read paths. Deliberately choose whether the result establishes remembered
window evidence through `client/windows.rs`. Add a view and renderer only when
needed. Check availability during Stop and other control, malformed responses,
and the actual helper observation. A look must not become mutation authority.

### Input action

Declare the shape and personal-input permission together in `core/src/actions.rs`.
Update whole-batch validation/cost, exhaustive target classification, keyboard
continuation choices and native execution in `native/src/native/events.rs`. A new held
state needs journal and release handling. CLI and viewer exposure are separate
deliberate choices. Compile errors cover closed alternatives; fresh guards,
recovery ordering and real target delivery still need review and helper proof.

### Daemon request

Add its variant/parser in `core/src/protocol.rs`, complete request policy,
execution in `daemon/src/daemon/serve.rs`, and verified client producer and result decoder.
Keep fresh off-console and session-pin checks around the operation. Mutations
also choose typed completion proof and pending ownership. Use an in-daemon read
when it suffices; do not introduce another worker executable by default.

### Native operation

Add the typed wire variant and its decode in `native/src/native/service/request.rs`, its
exhaustive dispatch choices and complete handler in `native/src/native/service.rs`, and
the client request. Decide cancellation, cleanup and registered-work lifetime
first. Preserve deferred field failures where existing guards must precede them.
Advertise a capability and require it on the client together when compatibility
needs it. Verify no input on refusal and retain unknown outcomes through dropped
requests and replies.

### MCP tool

Declare identity, catalog and exposure/authority policy together in
`cli/src/mcp/resources.rs`; add exhaustive execution in `cli/src/mcp.rs` and the
concrete handler beside its kind: `reads.rs` for what looks, `control.rs` for
the view's lease. Add a payload type only for actual fields. Verify the complete
catalog fixture, model/app visibility, guarded calls, privacy and real dispatch.
Declaring a tool is not proof that its app-control lifetime is correct.

### Viewer route

Declare its identity and path together in `routes!` in `cli/src/viewer/http.rs`, then
add execution and the semantic method in its owning control/read module. Add
validated mutation fields and make `Route::urgent` choose explicitly. Reuse
origin, body, changing guard and outcome boundaries. The execution and urgency
matches are exhaustive; owner-scoped cleanup and Stop/release under load still
need tests.

### Agent host

Add metadata in `client/src/agents.rs`, host-specific assets/integration, identity and
ordered sources in `core/src/host.rs`, ancestry/PID decisions and CLI
selection. Compiler errors cover closed identity choices; review complete lists,
host workflows and generated resources explicitly. Test install, removal,
refresh, ownership preservation and nested or ambiguous host sessions.

### Error code or reply field

Add a code and stable word together in `core/src/error.rs`, exhaustive advice in
`core/src/error/advice.rs`, its lease reading in `cli/src/viewer/control.rs`, its emission,
an independent stable-word fixture and docs.
For a reply field, update its typed schema/producer and each intended consumer
and privacy projection. Inspect both `core/src/transport.rs` and
`client/src/client/input.rs` refusal decoders. Unknown future fields need an explicit
compatibility decision; never forward arbitrary internal causes or request bodies.

### Setting

Update `core/src/config.rs`, defaults, validation/path normalization, the installed
constructor in `client/src/setup/configuration.rs`, its consumer, examples and fixtures.
The exhaustive destructuring in `Config::same_helper` requires a reload-identity
decision. Normalization and documentation are manual obligations. Test absent,
invalid, relative-path and reload behavior as applicable. Never manufacture a
helper identity through a production default just to simplify fixtures.

### Durable record

Choose its owner, key, model and local `Records` policy, then implement read,
write, removal, producer/consumer, retention and recovery integration. Reuse
`core/src/record_folder.rs`. Decide missing, corrupt, future, write-failure and
durability behavior before effects. Retention hooks and privacy projections
require explicit review; the compiler cannot prove them. Test crash/restart and
preservation of unknown ownership. Keep recovery records separate from public replies.

## Checks

Unit tests of private code live in a `tests.rs` beside their module, behind
`#[cfg(test)] mod tests;`. Tests through the public API live in `cli/tests/it/`,
which builds as one binary. Modules are `foo.rs` plus `foo/`, never `foo/mod.rs`.

Run `cargo fmt --all --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`,
`cargo +1.88.0 check --workspace --all-targets --locked`,
`cargo machete --skip-target-dir .`, `npm run format:check`, `npm test` and
`node scripts/check-swift.mjs` after relevant changes. The pinned checker install
is in `CONTRIBUTING.md`. Build both native targets for target-sensitive changes.

`cargo xtask quality --report` inventories review candidates; counts are not
defect counts or acceptance proof. Keep private inventories, measurements and
investigation evidence outside maintained source. Never ship a generated ledger
or make CI depend on a private ledger. Compare runtime behavior and agent cost
against a matching same-day baseline, preserving raw evidence and failed attempts.
