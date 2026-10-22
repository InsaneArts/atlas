import AppKit
import Foundation

/// Where a screenshot goes when the person copies it, and how its file is shown in the Finder. Handed to the
/// controller, so tests stand in for the clipboard and the Finder.
struct ShotOutlet {
  /// Puts a screenshot's PNG, as taken, on this Mac's clipboard.
  var copy: @MainActor (Data) -> Void
  /// Shows a screenshot's file in the Finder: only ever a listed screenshot inside the folder the service names
  /// (``ViewerShotsFacts/file(named:)``).
  var reveal: @MainActor (URL) -> Void

  /// The app's own: the general pasteboard, and the Finder selecting the file.
  @MainActor static var live: ShotOutlet {
    ShotOutlet(copy: copyToPasteboard) { file in
      NSWorkspace.shared.activateFileViewerSelecting([file])
    }
  }

  /// Puts a PNG on the general pasteboard, as a PNG and as an image for apps that take only TIFF.
  ///
  /// - Parameter png: The screenshot's bytes.
  @MainActor private static func copyToPasteboard(_ png: Data) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    if let image = NSImage(data: png) { pasteboard.writeObjects([image]) }
    pasteboard.addTypes([.png], owner: nil)
    pasteboard.setData(png, forType: .png)
  }
}
