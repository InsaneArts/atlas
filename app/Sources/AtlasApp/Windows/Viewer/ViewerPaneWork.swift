/// The tabs' working parts, unobserved: the one reading under way, the thumbnails being read, and the large view's.
struct ViewerPaneWork {
  /// Numbers each start and stop of a tab's reading, so a stopped one ends and its late reply is dropped.
  var generation = 0
  /// The tab's reading loop.
  var reading: Task<Void, Never>?
  /// The thumbnail queue's.
  var thumbnails = ThumbnailWork()
  /// The large view's.
  var large = LargeShotWork()
}
