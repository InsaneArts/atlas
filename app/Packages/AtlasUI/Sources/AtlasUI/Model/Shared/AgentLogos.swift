import Foundation
import ImageIO

/// The agents' original app icons, bundled for rows whose app is absent and for previews and renders.
public enum AgentLogos {
  /// The Claude logo shared by Claude Code and the Claude app.
  public static let claude = load("ClaudeLogo")
  /// The Codex logo shared by its command line and app.
  public static let codex = load("CodexLogo")
  /// The Cursor logo shared by its command line and app.
  public static let cursor = load("CursorLogo")

  /// Reads a required logo's Retina pixels from the app bundle, or beside the sources for previews and renders.
  ///
  /// - Parameter name: The resource's name without its scale or extension.
  /// - Returns: The original logo, ready for a row's image lead.
  private static func load(_ name: String) -> IconImage {
    let checkout = URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .appendingPathComponent("Resources/\(name)@2x.png")
    let url =
      Bundle.main
      .url(forResource: "\(name)@2x", withExtension: "png") ?? checkout
    guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
    else {
      preconditionFailure("Missing bundled agent logo: \(name)")
    }
    return IconImage(id: name, image: image)
  }
}
