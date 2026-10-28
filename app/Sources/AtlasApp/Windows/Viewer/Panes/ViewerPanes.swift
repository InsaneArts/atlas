/// What the Activity, Apps and Screenshots tabs hold, the thumbnails they share, and the host sessions' list they and
/// Details filter by. Observed, so a tab redraws with each reading.
struct ViewerPanes {
  /// The Activity tab's.
  var activity = ViewerActivityFacts()
  /// The Apps tab's.
  var apps = ViewerAppsFacts()
  /// The Screenshots tab's.
  var shots = ViewerShotsFacts()
  /// The screenshots' thumbnails read so far, shared by Activity, Screenshots and Details.
  var thumbnails = ThumbnailCache()
  /// The host sessions' list.
  var sessions = ViewerSessionsFacts()
}
