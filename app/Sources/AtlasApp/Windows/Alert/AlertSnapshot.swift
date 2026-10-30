import AtlasService

/// What the alert window's controller holds, as one plain value for ``AlertPresenter``.
struct AlertSnapshot {
  /// Which alert shows.
  var alert: HelperAlert
  /// Whether this process's session is on this Mac's console.
  var isOnConsole = false
  /// Whether its command runs now, so its buttons wait.
  var running = false
  /// Why its command did not run, shown in place of its sentence until it is pressed again; nil for none.
  var failure: String?
  /// Whether its command worked and the alert holds, saying so, before it closes.
  var done = false
  /// The service's latest overview, which says why Recover helps.
  var overview = ViewerStatus.Overview()
}
