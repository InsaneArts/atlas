import AtlasKit

/// Where logging the helper out stands: one value, since the confirmation and the offer to force a held log-out
/// never show together.
enum LogOutStage: Equatable, Sendable {
  /// Nothing is asked or offered.
  case idle
  /// Log Out Helper… was chosen and waits for the person to confirm.
  case confirming
  /// The last log-out was held up by an app in the helper; Log Out Anyway is offered.
  case held

  /// The stage a new status leaves: a confirmation, or the offer to force, only while the mode still allows it.
  ///
  /// - Parameter mode: The helper's mode, as the service now reports it.
  /// - Returns: This stage, or ``idle`` once the mode no longer allows it.
  func kept(in mode: HelperMode) -> LogOutStage {
    switch self {
    case .idle: .idle
    case .confirming: mode.allowsLogOut ? .confirming : .idle
    case .held: mode.allowsForcedLogOut ? .held : .idle
    }
  }
}
