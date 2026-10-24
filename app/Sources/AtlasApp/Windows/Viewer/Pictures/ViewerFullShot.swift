import AtlasUI
import Foundation

/// The screenshot shown large, as taken: its PNG, which Copy puts on the clipboard, and its picture, decoded once.
struct ViewerFullShot {
  /// The PNG's bytes.
  var png: Data
  /// The picture; nil when the bytes are not one.
  var picture: DesktopPicture?
}
