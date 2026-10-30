/// One decoded image or recorded miss, with the evidence it processed and its place in the bounded LRU cache.
nonisolated struct AppIconCached {
  /// The evidence processed, so only new live evidence bypasses this entry.
  var want: AppIconWant
  /// The decoded icon, or a recorded miss.
  var found: AppIconFound?
  /// The monotonically increasing last-use number for LRU eviction.
  var used: UInt64
}
