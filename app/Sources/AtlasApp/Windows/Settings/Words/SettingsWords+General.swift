/// The General tab's words: the picture's three choices, and the pictures of agent actions this Mac holds.
extension SettingsWords {
  /// The picture's choice that reads as often as the window reads.
  static let live = "Live"
  /// The picture's choice of one every five seconds.
  static let everyFiveSeconds = "Every 5 Seconds"
  /// The picture's choice of none.
  static let paused = "Paused"
  /// Delete All…'s alert's title.
  static let deletePicturesTitle = "Delete all pictures of agent actions?"
  /// Delete All…'s button.
  static let deleteAll = "Delete All"
  /// Delete All…'s button while it runs.
  static let deleting = "Deleting…"

  /// Delete All…'s alert's sentence.
  ///
  /// - Parameter count: How many pictures this Mac holds, when known.
  /// - Returns: "Atlas deletes the 1,240 pictures on this Mac. Activity keeps its steps, without pictures."
  static func deletePicturesMessage(_ count: Int?) -> String {
    let which = count.map { "the \(pictures($0))" } ?? "every picture"
    return "Atlas deletes \(which) on this Mac. Activity keeps its steps, without pictures."
  }

  /// How many pictures this Mac holds and their size.
  ///
  /// - Parameters:
  ///   - count: How many.
  ///   - bytes: Their size on disk.
  /// - Returns: "1,240 pictures · 142 MB", "No pictures · 2 KB" for what is left beside none, or "No pictures".
  static func storedPictures(count: Int, bytes: Int) -> String {
    guard count > 0 || bytes > 0 else { return "No pictures" }
    let size = Int64(bytes)
      .formatted(.byteCount(style: .file))
    return count > 0 ? "\(pictures(count)) · \(size)" : "No pictures · \(size)"
  }

  /// A count of pictures.
  ///
  /// - Parameter count: How many.
  /// - Returns: "1 picture", or "1,240 pictures".
  private static func pictures(_ count: Int) -> String {
    count == 1 ? "1 picture" : "\(count.formatted()) pictures"
  }
}
