/// The screenshot an Activity line took: its file name, which Open shows large, and its thumbnail once read.
package struct ViewerActivityShot: Equatable, Sendable {
  /// The screenshot's file name.
  package var name: String
  /// Its thumbnail; nil until it is read.
  package var thumbnail: DesktopPicture?

  /// Creates a line's screenshot.
  ///
  /// - Parameters:
  ///   - name: The screenshot's file name.
  ///   - thumbnail: Its thumbnail; nil until it is read.
  package init(name: String, thumbnail: DesktopPicture? = nil) {
    self.name = name
    self.thumbnail = thumbnail
  }
}
