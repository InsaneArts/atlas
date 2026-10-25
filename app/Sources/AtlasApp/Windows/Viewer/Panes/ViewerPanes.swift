/// What the Activity, Apps and Screenshots tabs hold, and the thumbnails they share. Observed, so a tab redraws with
/// each reading.
struct ViewerPanes {
  /// The Activity tab's.
  var activity = ViewerActivityFacts()
  /// The Apps tab's.
  var apps = ViewerAppsFacts()
  /// The Screenshots tab's.
  var shots = ViewerShotsFacts()
  /// The screenshots' thumbnails read so far, shared by Activity and Screenshots.
  var thumbnails = ThumbnailCache()
}
