import AtlasKit

@testable import AtlasApp

/// What a stand-in notifier and Dock badge were asked, and the choices Settings holds for them, so no test reaches
/// Notification Center, the Dock or the real defaults.
@MainActor
final class SignalLog {
  /// Whether Notify me is on.
  var notify = false
  /// Whether the Dock badge is on.
  var badge = false
  /// The notices posted, in order.
  var posted: [RecoveryNotice] = []
  /// The badge labels set, in order; nil clears it.
  var badges: [String?] = []

  /// Signals that post, badge and read their choices here.
  var signals: AttentionSignals {
    AttentionSignals(
      notifier: Notifier(requestAuthorization: {}, post: { self.posted.append($0) }),
      badge: DockBadge { self.badges.append($0) },
      preferences: SettingsPreferences(
        loadTab: { nil },
        saveTab: { _ in },
        loadNotify: { self.notify },
        saveNotify: { self.notify = $0 },
        loadBadge: { self.badge },
        saveBadge: { self.badge = $0 }
      )
    )
  }
}
