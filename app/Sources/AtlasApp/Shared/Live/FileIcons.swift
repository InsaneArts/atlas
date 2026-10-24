import AppKit

/// The icons files show, as the Finder draws them, read through the workspace: the one place Settings' agent
/// icons reach it.
enum FileIcons {
  /// 60: an icon's pixels, twice the 30-point row icon, so it stays sharp on a Retina display.
  static let pixels: CGFloat = 60

  /// The icon a file or an app shows, by its path.
  ///
  /// - Parameter path: The file's path.
  /// - Returns: Its icon at ``pixels``, or nil where no file is there.
  @MainActor static func icon(forFile path: String) -> CGImage? {
    guard FileManager.default.fileExists(atPath: path) else { return nil }
    var rect = CGRect(x: 0, y: 0, width: pixels, height: pixels)
    return NSWorkspace.shared
      .icon(forFile: path)
      .cgImage(forProposedRect: &rect, context: nil, hints: nil)
  }
}
