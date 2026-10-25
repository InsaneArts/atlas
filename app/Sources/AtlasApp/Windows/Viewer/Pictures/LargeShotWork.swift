/// The large view's working parts, unobserved: its picture being read and the copies under way.
struct LargeShotWork {
  /// Numbers each screenshot shown large and each hide, so a picture read before either never lands.
  var generation = 0
  /// The large screenshot's picture being read.
  var load: Task<Void, Never>?
  /// Numbers each Copy, so only the latest one reaches the clipboard.
  var copyGeneration = 0
  /// The latest Copy still reading its picture.
  var copy: Task<Void, Never>?
}
