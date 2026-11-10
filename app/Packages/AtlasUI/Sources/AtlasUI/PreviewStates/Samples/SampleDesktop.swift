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

    /// The sample chat app at a stage of posting a release note, as the picture before an agent's action shows it,
    /// named `id`; drawn once a stage and kept.
    ///
    /// - Parameters:
    ///   - id: The picture's id, such as its file name.
    ///   - stage: Where the task stands.
    /// - Returns: The picture, or nil should drawing it fail.
    @MainActor public static func chat(_ id: String, stage: SampleChatStage) -> DesktopPicture? {
      drawn(id, key: "chat-\(stage.rawValue)", scene: SampleChatScene(stage: stage))
    }

    /// The sample sign-in page, as the picture before an agent types a password shows it, named `id`.
    ///
    /// - Parameter id: The picture's id, such as its file name.
    /// - Returns: The picture, or nil should drawing it fail.
    @MainActor public static func signIn(_ id: String) -> DesktopPicture? {
      drawn(id, key: "sign-in", scene: SampleSignInScene())
    }

    /// A blank picture, black from edge to edge, as a screenshot of a locked or asleep display comes out, named `id`.
    ///
    /// - Parameter id: The picture's id, such as its file name.
    /// - Returns: The picture, or nil should drawing it fail.
    @MainActor public static func blank(_ id: String) -> DesktopPicture? {
      drawn(id, key: "blank", scene: Color.black.frame(width: 576, height: 360))
    }

    /// A scene drawn one pixel a point and kept under `key`, named `id`.
    ///
    /// - Parameters:
    ///   - id: The picture's id.
    ///   - key: What it is kept under.
    ///   - scene: The scene.
    /// - Returns: The picture, or nil should drawing it fail.
    @MainActor private static func drawn(_ id: String, key: String, scene: some View) -> DesktopPicture? {
      if let image = thumbnails[key] { return DesktopPicture(id: id, image: image) }
      let renderer = ImageRenderer(content: scene)
      renderer.scale = 1
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
