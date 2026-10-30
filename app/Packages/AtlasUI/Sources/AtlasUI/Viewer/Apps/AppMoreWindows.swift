import SwiftUI

/// The row under a card's windows when it lists only some: "N more", which opens a menu of the rest, each showing its
/// window on the screen, so the card never grows on a click.
struct AppMoreWindows: View {
  /// The app's process id.
  let app: Int
  /// The windows past those the card lists.
  let windows: [ViewerWindowRow]
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// "N more" as a menu.
  var body: some View {
    Menu {
      ForEach(windows) { window in
        Button(window.title) {
          send(.showWindow(app: app, window: window.id))
        }
        .accessibilityHint(Text(window.showLabel))
      }
    } label: {
      HStack(spacing: ViewerLayout.appCardWindowGlyphGap) {
        Image(systemName: SymbolName.more.rawValue)
          .font(Typography.font(.caption))
          .foregroundStyle(Color.textTertiary)
        Text(verbatim: windows.count == 1 ? "1 more window" : "\(windows.count) more windows")
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textSecondary)
          .lineLimit(1)
      }
    }
    .menuStyle(.button)
    .menuIndicator(.hidden)
    .buttonStyle(AppWindowRowStyle(isFront: false, offersShow: false))
    .help("Show another of this app's windows on the screen")
  }
}
