/// One decoded image or recorded miss, with what was looked up and its place in the bounded LRU cache.
nonisolated struct AppIconCached {
  /// What was looked up, whose name a found bundle identifier stands in for.
  var want: AppIconWant
  /// The decoded icon, or a recorded miss.
  var found: AppIconFound?
  /// The monotonically increasing last-use number for LRU eviction.
  var used: UInt64
}
