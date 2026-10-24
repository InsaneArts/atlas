import AppKit
import Foundation
import UniformTypeIdentifiers

/// Where a screenshot goes when the person copies or saves it, and how its file is shown in the Finder. Handed to the
/// controller, so tests stand in for the clipboard, the save panel and the Finder.
struct ShotOutlet {
  /// Puts a screenshot's PNG, as taken, on this Mac's clipboard.
  var copy: @MainActor (Data) -> Void
  /// Shows a screenshot's file in the Finder: only ever a listed screenshot inside the folder the service names
  /// (``ViewerShotsFacts/file(named:)``).
  var reveal: @MainActor (URL) -> Void
  /// Asks where to save a screenshot's PNG, suggesting its name, and writes it there: true once written, false when it
  /// could not be, nil when the person cancelled.
  var save: @MainActor (Data, String) -> Bool? = { _, _ in nil }

  /// The app's own: the general pasteboard, the Finder selecting the file, and a save panel.
  ///
  /// - Parameter workspace: The workspace that shows a file in the Finder; the app's container hands in the shared one.
  /// - Returns: The outlet.
  static func live(_ workspace: NSWorkspace) -> ShotOutlet {
    ShotOutlet(
      copy: copyToPasteboard,
      reveal: { file in workspace.activateFileViewerSelecting([file]) },
      save: saveWithPanel
    )
  }

  /// Puts a PNG on the general pasteboard, as a PNG and as an image for apps that take only TIFF.
  ///
  /// - Parameter png: The screenshot's bytes.
  private static func copyToPasteboard(_ png: Data) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    if let image = NSImage(data: png) { pasteboard.writeObjects([image]) }
    pasteboard.addTypes([.png], owner: nil)
    pasteboard.setData(png, forType: .png)
  }

  /// Asks where to save a PNG, then writes it there.
  ///
  /// - Parameters:
  ///   - png: The screenshot's bytes.
  ///   - name: The file's suggested name, without its extension.
  /// - Returns: Whether it was written, or nil when the person cancelled.
  private static func saveWithPanel(_ png: Data, name: String) -> Bool? {
    let panel = NSSavePanel()
    panel.allowedContentTypes = [.png]
    panel.nameFieldStringValue = name
    guard panel.runModal() == .OK, let url = panel.url else { return nil }
    return (try? png.write(to: url, options: .atomic)) != nil
  }
}
