import SwiftUI

/// One of an app's windows on its card: a window's glyph and its title on a 28-point capsule, the whole row Show on
/// Screen, with "Show" at its end under the pointer. The front window of the app in front rests on a quiet fill.
struct AppWindowRow: View {
  /// The window.
  let window: ViewerWindowRow
  /// Shows the window on the screen.
  let show: @MainActor () -> Void

  /// The row as a button.
  var body: some View {
    Button(action: show) {
      HStack(spacing: ViewerLayout.appCardWindowGlyphGap) {
        Image(systemName: SymbolName.window.rawValue)
          .font(Typography.font(.caption))
          .foregroundStyle(Color.textTertiary)
        Text(window.title)
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textPrimary)
          .lineLimit(1)
          .truncationMode(.tail)
      }
    }
    .buttonStyle(AppWindowRowStyle(isFront: window.isFront))
    .help("Outline this window on the screen")
    .accessibilityLabel(Text(verbatim: "\(window.title), \(window.size)"))
    .accessibilityHint(Text(window.showLabel))
  }
}

#if DEBUG
  #Preview("Samples") {
    AppsSamples(.windows)
  }
#endif
