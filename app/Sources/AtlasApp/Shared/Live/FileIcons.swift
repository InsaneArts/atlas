import AppKit

/// The icons files show, as the Finder draws them, read through the workspace: the one place Settings' agent
/// and account icons reach it.
enum FileIcons {
  /// 60: an icon's pixels, twice the 30-point row icon, so it stays sharp on a Retina display.
  static let pixels: CGFloat = 60
  /// 128: an account's app icon's pixels, so it stays sharp on a Retina display on its page's 42-point tile.
  static let appPixels: CGFloat = 128

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

  /// The icon of the app whose bundle identifier is `bundleID`, where it is on this Mac, as an account of the helper's
  /// shows it.
  ///
  /// - Parameter bundleID: The app's bundle identifier.
  /// - Returns: Its icon at ``appPixels``, or nil where the app isn't on this Mac.
  @MainActor static func icon(forApp bundleID: String) -> CGImage? {
    guard let url = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleID) else { return nil }
    var rect = CGRect(x: 0, y: 0, width: appPixels, height: appPixels)
    return NSWorkspace.shared
      .icon(forFile: url.path)
      .cgImage(forProposedRect: &rect, context: nil, hints: nil)
  }
}
