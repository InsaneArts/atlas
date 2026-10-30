/// Where a ``LineSwap`` stands: the one line it draws, the newest line waiting to replace it, and what the drawn line
/// is doing. Lines are known by their ids, so the same line with new details, such as its age, is never a swap. The
/// rule is pure, so it can be followed step by step without a clock:
/// - A line holds for at least ``Motion/Duration/lineHold`` from the moment it starts to fade in. A line that arrives
///   during the hold waits for it to end. Action labels skip the hold so obsolete words yield immediately.
/// - Once the hold is over, a waiting line, or one that arrives later, starts the drawn line fading out; only once it
///   has faded out does the waiting line fade in. The two are never drawn together.
/// - Only the newest line waits: one that arrives while another waits replaces it, and lines in between are skipped.
/// - The drawn line coming back while it fades out turns its fade round, from where it is, and holds again.
struct LineSwapQueue<ID: Equatable>: Equatable {
  /// What the drawn line is doing.
  enum Phase: Equatable, Sendable {
    /// Fading in or shown, and within its hold, when enabled.
    case holding
    /// Shown, its hold over: the next line replaces it at once.
    case resting
    /// Fading out, so the waiting line can fade in once it has gone.
    case fadingOut
  }

  /// What happened to the swap.
  enum Event: Equatable {
    /// A line arrived, by its id: a new one, or the drawn one with new details.
    case arrived(ID)
    /// The drawn line's hold ended.
    case held
    /// The drawn line finished fading out.
    case fadedOut
  }

  /// The line drawn now.
  let shown: ID
  /// The newest line waiting to replace it, if any.
  let waiting: ID?
  /// What the drawn line is doing.
  let phase: Phase

  /// A swap drawing `shown`, its hold over, so the first change shows at once.
  ///
  /// - Parameter shown: The line drawn first.
  init(shown: ID) {
    self.init(shown: shown, waiting: nil, phase: .resting)
  }

  /// A swap in any state.
  ///
  /// - Parameters:
  ///   - shown: The line drawn now.
  ///   - waiting: The newest line waiting to replace it.
  ///   - phase: What the drawn line is doing.
  init(shown: ID, waiting: ID?, phase: Phase) {
    self.shown = shown
    self.waiting = waiting
    self.phase = phase
  }

  /// The swap after `event`.
  ///
  /// - Parameters:
  ///   - event: What happened.
  ///   - holds: Whether lines keep a minimum reading time. Action labels must yield immediately.
  /// - Returns: The swap's next state; a change of ``phase`` to ``Phase/fadingOut`` starts a fade out, and one to
  ///   ``Phase/holding`` starts a fade in of ``shown`` and its hold.
  func next(_ event: Event, holds: Bool = true) -> LineSwapQueue {
    switch (event, phase) {
    case (.arrived(let id), .holding):
      LineSwapQueue(shown: shown, waiting: id == shown ? nil : id, phase: !holds && id != shown ? .fadingOut : .holding)
    case (.arrived(let id), .resting):
      id == shown ? self : LineSwapQueue(shown: shown, waiting: id, phase: .fadingOut)
    case (.arrived(let id), .fadingOut):
      id == shown
        ? LineSwapQueue(shown: shown, waiting: nil, phase: .holding)
        : LineSwapQueue(shown: shown, waiting: id, phase: .fadingOut)
    case (.held, .holding):
      LineSwapQueue(shown: shown, waiting: waiting, phase: waiting == nil ? .resting : .fadingOut)
    case (.fadedOut, .fadingOut):
      LineSwapQueue(shown: waiting ?? shown, waiting: nil, phase: .holding)
    case (.held, _), (.fadedOut, _):
      self
    }
  }
}
