/// What the Activity pane holds, its thumbnails and app icons, and the host sessions' list it and the sidebar filter
/// by. Observed, so the pane redraws with each reading.
struct ViewerPanes {
  /// The Activity pane's.
  var activity = ViewerActivityFacts()
  /// The screenshots' list, which sizes the outline's pictures and finds their files.
  var shots = ViewerShotsFacts()
  /// The screenshots' thumbnails read so far, shared by Activity's outline, its picture sheet and its replay.
  var thumbnails = ThumbnailCache()
  /// The apps' icons found so far, which Activity's rows draw.
  var icons = ViewerIconsFacts()
  /// The host sessions' list.
  var sessions = ViewerSessionsFacts()
}
