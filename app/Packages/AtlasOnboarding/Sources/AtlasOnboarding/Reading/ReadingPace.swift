/// When the setup window reads its facts next. Each reading starts processes, so the window reads when something
/// may have changed, watches closely for a moment after, and otherwise only now and then. What macOS reports never
/// brings readings closer than ``minimumGap``; the window's own ask, after one of its commands or a press, reads as
/// soon as the reading under way ends. Times are how long the window has been open, so the rule needs no clock.
public struct ReadingPace: Equatable, Sendable {
  /// The least time between the starts of two readings: 1.5 s.
  public static let minimumGap: Duration = .milliseconds(1500)
  /// How long after a change the window reads at ``minimumGap``: 10 s, as what follows a change takes a moment (the
  /// Dock comes up after a login, a job restarts after a grant).
  public static let closeWatch: Duration = .seconds(10)
  /// The time between readings while nothing changes: 10 s.
  public static let fallbackEvery: Duration = .seconds(10)

  /// When the last reading started; nil before the first.
  private var lastStart: Duration?
  /// Whether something may have changed since the last reading started.
  private var changed = false
  /// Whether the window asked for a reading since the last one started.
  private var asked = false
  /// When something last may have changed.
  private var lastChange: Duration?
  /// The times asked to read at.
  private var deadlines: [Deadline: Duration] = [:]

  /// A pace that reads at once.
  public init() {}

  /// When the next reading starts: at once for the first and once the window asked; otherwise never before
  /// ``minimumGap`` after the last, and then as soon as a change came in, at ``minimumGap`` within ``closeWatch`` of
  /// the latest change or ask, at the earliest deadline, or ``fallbackEvery`` after the last.
  public var next: Duration {
    guard let lastStart else { return .zero }
    if asked { return lastStart }
    let earliest = lastStart + Self.minimumGap
    var due = lastStart + Self.fallbackEvery
    if changed { due = earliest }
    if let lastChange, lastStart < lastChange + Self.closeWatch { due = min(due, earliest) }
    if let soonest = deadlines.values.min() { due = min(due, soonest) }
    return max(earliest, due)
  }

  /// Something may have changed at `now`, as macOS reports.
  ///
  /// - Parameter now: The time.
  public mutating func change(at now: Duration) {
    changed = true
    lastChange = now
  }

  /// The window asks for a reading at `now`, after one of its commands or a press: what follows may take a moment,
  /// so the window watches closely after it too.
  ///
  /// - Parameter now: The time.
  public mutating func ask(at now: Duration) {
    asked = true
    lastChange = now
  }

  /// A reading starts at `now`: it covers every change so far and every deadline passed.
  ///
  /// - Parameter now: The time.
  public mutating func start(at now: Duration) {
    lastStart = now
    changed = false
    asked = false
    deadlines = deadlines.filter { $0.value > now }
  }

  /// Asks for a reading at `time`, for `reason`, in place of any earlier ask for it.
  ///
  /// - Parameters:
  ///   - reason: Why.
  ///   - time: When.
  public mutating func read(_ reason: Deadline, at time: Duration) {
    deadlines[reason] = time
  }

  /// Drops the ask for `reason`.
  ///
  /// - Parameter reason: Why it was asked.
  public mutating func drop(_ reason: Deadline) {
    deadlines[reason] = nil
  }
}
