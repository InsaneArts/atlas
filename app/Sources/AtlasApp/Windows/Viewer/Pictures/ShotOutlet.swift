import AppKit
import Foundation
import UniformTypeIdentifiers

/// Where a screenshot goes when the person copies or saves it, and how its file is shown in the Finder. Handed to the
/// controller, so tests stand in for the clipboard, the save panel and the Finder.
struct ShotOutlet {
  /// Puts a picture's PNG, as taken, on this Mac's clipboard, with its file when it has one: true once it is there.
  var copy: @MainActor (Data, URL?) -> Bool
  /// Shows a picture's file in the Finder: only ever a listed screenshot, or a kept picture, inside the folder the
  /// service names (``ScreenshotList/pictureFile(named:)``); true once shown, false when the file is not there.
  var reveal: @MainActor (URL) -> Bool
  /// Asks where to save a screenshot's PNG, suggesting its name, and writes it there: true once written, false when it
  /// could not be, nil when the person cancelled.
  var save: @MainActor (Data, String) -> Bool? = { _, _ in nil }

  /// The app's own: the general pasteboard, the Finder selecting the file, and a save panel.
  ///
  /// - Parameters:
  ///   - workspace: The workspace that shows a file in the Finder; the app's container hands in the shared one.
  ///   - pasteboard: The pasteboard Copy writes to; the general one, which tests replace with one of their own.
  /// - Returns: The outlet.
  static func live(_ workspace: NSWorkspace, pasteboard: NSPasteboard = .general) -> ShotOutlet {
    ShotOutlet(
      copy: { png, file in copy(png, file: file, to: pasteboard) },
      reveal: { file in
        guard FileManager.default.fileExists(atPath: file.path) else { return false }
        workspace.activateFileViewerSelecting([file])
        return true
      },
      save: saveWithPanel
    )
  }

  /// Puts a PNG on `pasteboard` as one item that pastes anywhere: as PNG, as TIFF for apps that take only that, and as
  /// the picture's file for the Finder and apps that take files, when the file is there.
  ///
  /// - Parameters:
  ///   - png: The picture's bytes.
  ///   - file: The picture's file, if it has one.
  ///   - pasteboard: The pasteboard.
  /// - Returns: Whether the picture is on the pasteboard.
  static func copy(_ png: Data, file: URL?, to pasteboard: NSPasteboard) -> Bool {
    guard let image = NSImage(data: png), let tiff = image.tiffRepresentation else { return false }
    let item = NSPasteboardItem()
    var written = item.setData(png, forType: .png) && item.setData(tiff, forType: .tiff)
    if let file, file.isFileURL, FileManager.default.fileExists(atPath: file.path) {
      written = written && item.setString(file.absoluteString, forType: .fileURL)
    }
    guard written else { return false }
    pasteboard.clearContents()
    return pasteboard.writeObjects([item])
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
