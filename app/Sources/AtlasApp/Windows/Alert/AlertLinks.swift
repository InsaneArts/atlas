import AtlasKit

/// What the alert window asks of the rest of the app, as closures `AppController` gives it at `init`.
struct AlertLinks {
  /// The service's latest overview, as the menu bar panel last read it.
  var overview: @MainActor () -> ViewerStatus.Overview = { ViewerStatus.Overview() }
  /// Reads the status again, so the menu bar follows what a command did.
  var readStatus: @MainActor () async -> Void = {}
  /// Switches this Mac's screen to the helper, finding it first: its outcome, or nil when a switch already runs.
  var switchToHelper: @MainActor () async -> Outcome? = { nil }
  /// Closes the alert window, if it still shows the given controller's alert: an older one never closes a newer.
  var close: @MainActor (AlertController) -> Void = { _ in }
}
