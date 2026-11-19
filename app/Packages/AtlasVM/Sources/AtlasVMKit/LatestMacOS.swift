import Foundation

/// The newest macOS Apple's framework supports on this Mac, as a VM's
/// station is told it: asked of the framework at most once a day, the
/// answer kept in a file every run shares; an asking that failed is tried
/// again after an hour, and says none meanwhile, which is never taken for
/// the release a VM already runs.
public actor LatestMacOS {
  /// How long a release named is kept before the framework is asked again.
  public static let keeping: TimeInterval = 24 * 60 * 60
  /// How long a failed asking is kept before it is tried again.
  public static let retrying: TimeInterval = 60 * 60

  private let file: URL
  private let now: @Sendable () -> Date
  private let ask: @Sendable () async throws -> MacOSRelease
  /// The answer last kept, once read.
  private var kept: LatestKept?
  /// The asking under way, which every request meanwhile waits for.
  private var asking: Task<LatestKept, Never>?

  /// The newest release `ask` names, its answer kept at `file`, its time
  /// told by `now`.
  public init(
    file: URL,
    now: @escaping @Sendable () -> Date = { Date() },
    ask: @escaping @Sendable () async throws -> MacOSRelease
  ) {
    self.file = file
    self.now = now
    self.ask = ask
  }

  /// The newest release, as kept or asked now; none when it cannot be told.
  public func release() async -> MacOSRelease? {
    if kept == nil {
      kept = try? LatestKept.read(from: file)
    }
    if let kept, fresh(kept) {
      return kept.release
    }
    if let asking {
      return await asking.value.release
    }
    let (ask, now) = (ask, now)
    let task = Task<LatestKept, Never> {
      let release = try? await ask()
      return LatestKept(release: release, asked: now())
    }
    asking = task
    let answer = await task.value
    asking = nil
    kept = answer
    try? answer.write(to: file)
    return answer.release
  }

  /// Whether `kept` is recent enough to answer with: never one asked later
  /// than now, as a clock set back leaves.
  private func fresh(_ kept: LatestKept) -> Bool {
    let age = now()
      .timeIntervalSince(kept.asked)
    return age >= 0 && age < (kept.release == nil ? Self.retrying : Self.keeping)
  }
}
