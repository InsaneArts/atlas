/// Returns once every job queued on the main actor before the call has run to its first wait: a sentinel a test sends
/// itself to prove an absence, queued behind the work that must produce nothing.
///
/// It orders only jobs queued on the main actor. Atlas's own seams, such as `any CommandRunning`, run on their caller's
/// actor, so a main-actor fake answers in line. A call through a protocol the standard library declares, as a sleep
/// through `any Clock<Duration>` is, hops off the actor and back, and can land after it; an absence there ends on
/// something positive through the same path instead. It is no pause: it ends as soon as the jobs ahead of it have run.
@MainActor
public func afterQueuedWork() async {
  // Unstructured on purpose: a new main-actor job, queued behind every one before it.
  await Task { @MainActor in }.value
}
