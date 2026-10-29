import AtlasService

/// What the Apps tab last read and the window shown on the screen. The sidebar's search narrows it. No picture of a
/// window is kept here: a card's is cut from the live picture each time the pane is drawn.
struct ViewerAppsFacts {
  /// The apps, front to back; nil before the first reading.
  var apps: [HelperApp]?
  /// The host sessions the apps' origins name, by key.
  var sessions: [String: HostSessionSummary] = [:]
  /// The windows that stood in the same place and at the same size in the last two readings, by number, whose
  /// pictures may be cut from the live picture: one still moving may not match the frame.
  var steadyWindows: Set<Int> = []
  /// Why the last reading failed, while it has; nil once one worked.
  var failure: String?
  /// The window outlined on the screen; nil shows none.
  var shown: ViewerShownWindow?
}
