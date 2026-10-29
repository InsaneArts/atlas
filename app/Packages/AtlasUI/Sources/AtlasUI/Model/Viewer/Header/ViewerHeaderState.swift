/// The header over the helper's screen: who has the helper, such as "Claude Code · understudy" and "has the helper",
/// "You" and "have control of the helper", or the helper's state alone.
public struct ViewerHeaderState: Equatable, Sendable {
  /// Who, or the helper's state, in the header's semibold.
  public var who: String
  /// What they do with it, after them in grey; empty for none.
  public var what: String
  /// How ``who`` reads: orange when the helper needs the person or agents are stopped, plain otherwise.
  public var tone: StatusTone
  /// Whether ``what`` shows: not in a narrow card, where ``who`` takes the line.
  public var showsWhat: Bool

  /// The words the header draws, who and what as they show, which tell one line from the next: new words swap the
  /// line, a new tone for the same words lands in place.
  var words: [String] {
    [who, showsWhat ? what : ""]
  }

  /// Creates the header's state.
  ///
  /// - Parameters:
  ///   - who: Who, or the helper's state.
  ///   - what: What they do with it; none by default.
  ///   - tone: How `who` reads; quiet by default.
  ///   - showsWhat: Whether `what` shows; yes by default.
  public init(who: String, what: String = "", tone: StatusTone = .quiet, showsWhat: Bool = true) {
    self.who = who
    self.what = what
    self.tone = tone
    self.showsWhat = showsWhat
  }
}
