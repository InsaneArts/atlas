import Foundation

/// The tabs' working parts, unobserved: the one reading under way, the thumbnails being read, the large view's, and
/// the host sessions' list's reading.
struct ViewerPaneWork {
  /// Numbers each start and stop of a tab's reading, so a stopped one ends and its late reply is dropped.
  var generation = 0
  /// The tab's reading loop.
  var reading: Task<Void, Never>?
  /// The thumbnail queue's.
  var thumbnails = ThumbnailWork()
  /// The large view's.
  var large = LargeShotWork()
  /// Numbers each start and stop of the sessions' list's reading, so a stopped one ends and its late reply is dropped.
  var sessionsGeneration = 0
  /// The sessions' list's reading loop, while the window wants the list.
  var sessions: Task<Void, Never>?
  /// Whether the sessions task is polling live sessions rather than discovering history once.
  var sessionsPolling = false
  /// When history was last requested; status readings refresh it without adding another clock sleeper.
  var sessionsReadAt: Date?
}
