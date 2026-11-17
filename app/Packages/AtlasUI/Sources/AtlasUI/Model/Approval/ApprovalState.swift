/// What the approval window shows: what a command asks the person to allow, for which VM, from which session, and
/// where the answer stands.
public struct ApprovalState: Equatable, Sendable {
  /// What it asks; nil until the request is read.
  public var operation: ApprovalOperation?
  /// The VM's name, such as `work`.
  public var desktop: String
  /// The session that asked, in a few words, such as "Claude Code in atlas".
  public var session: String
  /// Where the answer stands.
  public var phase: ApprovalPhase

  /// Creates the window's state.
  ///
  /// - Parameters:
  ///   - operation: What it asks; nil until the request is read.
  ///   - desktop: The VM's name.
  ///   - session: The session that asked.
  ///   - phase: Where the answer stands.
  public init(operation: ApprovalOperation?, desktop: String, session: String, phase: ApprovalPhase) {
    self.operation = operation
    self.desktop = desktop
    self.session = session
    self.phase = phase
  }

  /// The window while its request is read.
  public static let loading = ApprovalState(operation: nil, desktop: "", session: "", phase: .loading)
}
