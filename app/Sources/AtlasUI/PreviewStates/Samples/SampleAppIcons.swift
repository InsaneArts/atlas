#if DEBUG
  import SwiftUI

  /// Stand-in app icons for the Apps samples: a rounded tile in an app's colours with a symbol on it, drawn once.
  /// Their colours and sizes are a sample's, not the app's, so they are not tokens.
  package enum SampleAppIcons {
    /// The samples' apps, by the bundle path their icon is known by.
    @MainActor package static let icons: [String: IconImage] = {
      let drawn: [String: IconImage?] = [
        opie: icon(opie, symbol: "circle.circle.fill", colors: [.black, Color(white: 0.2)]),
        safari: icon(safari, symbol: "safari.fill", colors: [.white, Color(white: 0.85)], ink: .blue),
        notes: icon(notes, symbol: "list.bullet", colors: [.yellow, .white], ink: .gray),
        terminal: icon(terminal, symbol: "chevron.right", colors: [Color(white: 0.15), .black]),
        finder: icon(finder, symbol: "face.smiling", colors: [.cyan, .blue]),
      ]
      return drawn.compactMapValues { $0 }
    }()

    /// Opie's bundle path.
    package static let opie = "/Applications/Opie.app"
    /// Safari's bundle path.
    package static let safari = "/Applications/Safari.app"
    /// Notes' bundle path.
    package static let notes = "/System/Applications/Notes.app"
    /// Terminal's bundle path.
    package static let terminal = "/System/Applications/Utilities/Terminal.app"
    /// Finder's bundle path.
    package static let finder = "/System/Library/CoreServices/Finder.app"

    /// One stand-in icon.
    ///
    /// - Parameters:
    ///   - id: The bundle path it is known by.
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
      let tile = RoundedRectangle(cornerRadius: 15, style: .continuous)
        .fill(LinearGradient(colors: colors, startPoint: .top, endPoint: .bottom))
        .overlay(
          Image(systemName: symbol)
            .font(.system(size: 34, weight: .semibold))
            .foregroundStyle(ink)
        )
        .frame(width: 64, height: 64)
        .padding(4)
      let renderer = ImageRenderer(content: tile)
      renderer.scale = 2
      return renderer.cgImage.map { IconImage(id: id, image: $0) }
    }
  }
#endif
