/// The thumbnail queue's working parts, unobserved.
struct ThumbnailWork {
  /// Numbers each reset of the queue, so a reply from before one never lands.
  var generation = 0
  /// Thumbnails asked for and not yet started, oldest first.
  var wanted: [String] = []
  /// Thumbnails being read, by name.
  var loading: [String: Task<Void, Never>] = [:]
  /// Whether new reads wait, as while the window is being resized; those under way go on.
  var isHeld = false
}
