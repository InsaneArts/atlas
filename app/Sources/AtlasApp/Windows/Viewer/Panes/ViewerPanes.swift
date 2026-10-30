/// What the Activity, Apps and Screenshots panes hold, the thumbnails and app icons they share, and the host sessions'
/// list they and the sidebar filter by. Observed, so a pane redraws with each reading.
struct ViewerPanes {
  /// The Activity pane's.
  var activity = ViewerActivityFacts()
  /// The Apps pane's.
  var apps = ViewerAppsFacts()
  /// The Screenshots pane's.
  var shots = ViewerShotsFacts()
  /// The screenshots' thumbnails read so far, shared by Activity and Screenshots.
  var thumbnails = ThumbnailCache()
  /// The apps' icons found so far, shared by Activity and Apps.
  var icons = ViewerIconsFacts()
  /// The host sessions' list.
  var sessions = ViewerSessionsFacts()
}
