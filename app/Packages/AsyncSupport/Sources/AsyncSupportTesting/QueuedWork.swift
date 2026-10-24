/// Returns once every job queued on the main actor before the call has run to its first wait: a sentinel a test sends
/// itself to prove an absence, queued behind the work that must produce nothing.
///
/// It orders only jobs queued on the main actor. Work that goes through a nonisolated protocol call, as a command run
/// through `any CommandRunning` or a sleep through `any Clock<Duration>` does, hops off the actor and back, and can land
/// after it; an absence there ends on something positive through the same path instead. It is no pause: it ends as
/// soon as the jobs ahead of it have run.
@MainActor
public func afterQueuedWork() async {
  // Unstructured on purpose: a new main-actor job, queued behind every one before it.
  await Task { @MainActor in }.value
}
