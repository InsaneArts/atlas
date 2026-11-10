import AtlasService
import AtlasUI

/// The words for what let an agent's action act in one of the person's listed accounts, which Activity's steps and
/// runs and the panel's Recent share.
extension Words {
  /// The person allowed the rest of the agent's task in the account.
  static let allowedForTask = "You allowed for this task"
  /// The person allowed this one action.
  static let allowedOnce = "You allowed once"
  /// The account is Trusted, so nothing needed allowing.
  static let trusted = "Trusted"

  /// The badge for what let an action act in its account: the person's own allowing in the accent, Trusted quiet.
  ///
  /// - Parameter allowed: What let it act there, or nil when nothing needed allowing.
  /// - Returns: The badge, or nil for none.
  static func allowance(_ allowed: ActivityLog.Allowance?) -> AllowanceBadge? {
    switch allowed {
    case .task: AllowanceBadge(words: allowedForTask, tone: .accent)
    case .once: AllowanceBadge(words: allowedOnce, tone: .accent)
    case .trusted: AllowanceBadge(words: trusted, tone: .quiet)
    case nil: nil
    }
  }
}
