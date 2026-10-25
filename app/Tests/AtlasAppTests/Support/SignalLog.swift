import AtlasStatus
import Observation

@testable import AtlasApp

/// What a stand-in notifier was asked, and the choice Settings holds for it, so no test reaches Notification Center
/// or the real defaults.
@Observable
final class SignalLog {
  /// Whether Notify me is on.
  var notify = false
  /// The notices posted, in order.
  var posted: [RecoveryNotice] = []
  /// How often The helper needs you was taken out of Notification Center.
  var withdrawn = 0
  /// How often The helper logged out was taken out of Notification Center.
  var withdrawnLoggedOut = 0

  /// Signals that post and read their choice here.
  var signals: AttentionSignals {
    AttentionSignals(
      notifier: Notifier(
        requestAuthorization: {},
        post: { self.posted.append($0) },
        withdraw: { self.withdrawn += 1 },
        withdrawLoggedOut: { self.withdrawnLoggedOut += 1 }
      ),
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { self.notify },
        saveNotify: { self.notify = $0 }
      )
    )
  }
}
