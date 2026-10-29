#if DEBUG
  import SwiftUI

  /// A stand-in desktop for the panel's and the viewer's samples: a window on a blue ground, drawn once at a
  /// display's size, one pixel a point. Its colours, sizes and words are a sample desktop's, not the app's, so they
  /// are not tokens.
  public enum SampleDesktop {
    /// The desktop as a picture, or nil should drawing it fail.
    @MainActor public static let picture: DesktopPicture? = {
      let renderer = ImageRenderer(content: SampleDesktopScene())
      renderer.scale = 1
      return renderer.cgImage.map { DesktopPicture(id: "sample-desktop", image: $0) }
    }()

    /// The desktop drawn at another display's shape, or dark, small as a thumbnail is, named `id`; drawn once a
    /// shape and kept.
    ///
    /// - Parameters:
    ///   - id: The picture's id, such as a screenshot's file name.
    ///   - display: The display's size, in points.
    ///   - isDark: Whether it is drawn dark.
    /// - Returns: The picture, or nil should drawing it fail.
    @MainActor public static func thumbnail(_ id: String, display: CGSize, isDark: Bool = false) -> DesktopPicture? {
      let key = "\(Int(display.width))x\(Int(display.height))\(isDark ? "-dark" : "")"
      if let image = thumbnails[key] { return DesktopPicture(id: id, image: image) }
      let renderer = ImageRenderer(content: SampleDesktopScene(display: display, isDark: isDark))
      renderer.scale = thumbnailScale
      guard let image = renderer.cgImage else { return nil }
      thumbnails[key] = image
      return DesktopPicture(id: id, image: image)
    }

    /// 0.4: how small a thumbnail is drawn, about as the app reads them.
    private static let thumbnailScale: CGFloat = 0.4
    /// The thumbnails drawn so far, by shape.
    @MainActor private static var thumbnails: [String: CGImage] = [:]
  }
#endif
