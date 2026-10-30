#if DEBUG
  import SwiftUI

  /// Stand-in app icons for the Apps samples: a rounded tile in an app's colours with a symbol on it, on a real icon's
  /// grid (a body of 824 of the canvas's 1024 points), drawn once. Their colours and sizes are a sample's, not the
  /// app's, so they are not tokens.
  public enum SampleAppIcons {
    /// The samples' apps' icons, by bundle identifier.
    @MainActor public static let icons: [String: IconImage] = {
      let drawn: [String: IconImage?] = [
        opie: icon(opie, symbol: "circle.circle.fill", colors: [.black, Color(white: 0.2)]),
        safari: icon(safari, symbol: "safari.fill", colors: [.white, Color(white: 0.85)], ink: .blue),
        notes: icon(notes, symbol: "list.bullet", colors: [.yellow, .white], ink: .gray),
        terminal: icon(terminal, symbol: "chevron.right", colors: [Color(white: 0.15), .black]),
        finder: icon(finder, symbol: "face.smiling", colors: [.cyan, .blue]),
        textEdit: icon(textEdit, symbol: "pencil", colors: [.white, Color(white: 0.88)], ink: .gray),
      ]
      return drawn.compactMapValues { $0 }
    }()

    /// Opie's bundle identifier.
    public static let opie = "com.insanearts.opie"
    /// Safari's bundle identifier.
    public static let safari = "com.apple.Safari"
    /// Notes' bundle identifier.
    public static let notes = "com.apple.Notes"
    /// Terminal's bundle identifier.
    public static let terminal = "com.apple.Terminal"
    /// Finder's bundle identifier.
    public static let finder = "com.apple.finder"
    /// TextEdit's bundle identifier.
    public static let textEdit = "com.apple.TextEdit"

    /// One stand-in icon.
    ///
    /// - Parameters:
    ///   - id: The bundle identifier it is known by.
    ///   - symbol: The symbol on it.
    ///   - colors: Its tile's gradient, top first.
    ///   - ink: The symbol's colour.
    /// - Returns: The icon, or nil should drawing fail.
    @MainActor private static func icon(
      _ id: String,
      symbol: String,
      colors: [Color],
      ink: Color = .white
    ) -> IconImage? {
      let tile = RoundedRectangle(cornerRadius: 13, style: .continuous)
        .fill(LinearGradient(colors: colors, startPoint: .top, endPoint: .bottom))
        .overlay(
          Image(systemName: symbol)
            .font(.system(size: 30, weight: .semibold))
            .foregroundStyle(ink)
        )
        .frame(width: 58, height: 58)
        .frame(width: 72, height: 72)
      let renderer = ImageRenderer(content: tile)
      renderer.scale = 2
      return renderer.cgImage.map { IconImage(id: id, image: $0) }
    }
  }
#endif
