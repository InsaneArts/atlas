import AppKit
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

/// Where the picture on screen goes when the person copies or saves it: never a new capture, only the frame already
/// shown. Handed to the controller, so tests stand in for the clipboard and the save panel.
struct PictureOutlet {
  /// What a saved picture's file is called before the person names it.
  static let suggestedName = "Atlas helper.png"

  /// Puts a picture on this Mac's clipboard.
  var copy: @MainActor (CGImage) -> Void
  /// Asks where to save a picture and writes it there as a PNG: true once written, false when it could not be, nil
  /// when the person cancelled.
  var save: @MainActor (CGImage) -> Bool?

  /// The app's own: the general pasteboard and a save panel.
  static var live: PictureOutlet {
    PictureOutlet(copy: copyToPasteboard, save: saveWithPanel)
  }

  /// Puts `image` on the general pasteboard.
  ///
  /// - Parameter image: The picture.
  private static func copyToPasteboard(_ image: CGImage) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.writeObjects([NSImage(cgImage: image, size: .zero)])
  }

  /// Asks where to save `image`, then writes it as a PNG.
  ///
  /// - Parameter image: The picture.
  /// - Returns: Whether it was written, or nil when the person cancelled.
  private static func saveWithPanel(_ image: CGImage) -> Bool? {
    let panel = NSSavePanel()
    panel.allowedContentTypes = [.png]
    panel.nameFieldStringValue = suggestedName
    guard panel.runModal() == .OK, let url = panel.url else { return nil }
    return write(image, to: url)
  }

  /// Writes `image` as a PNG at `url`.
  ///
  /// - Parameters:
  ///   - image: The picture.
  ///   - url: Where.
  /// - Returns: Whether it was written.
  static func write(_ image: CGImage, to url: URL) -> Bool {
    guard
      let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)
    else { return false }
    CGImageDestinationAddImage(destination, image, nil)
    return CGImageDestinationFinalize(destination)
  }
}
