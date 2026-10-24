import SwiftUI

/// One of an app's windows under its name, over a hairline: its title, its size, and Show on screen.
struct AppWindowRow: View {
  /// The window.
  let window: ViewerWindowRow
  /// Shows the window on the screen.
  let show: @MainActor () -> Void

  var body: some View {
    HStack(spacing: Spacing.sPlus) {
      Text(window.title)
        .font(Typography.font(.body))
        .lineLimit(1)
      Text(window.size)
        .font(Typography.font(.callout))
        .foregroundStyle(Color.textTertiary)
        .monospacedDigit()
        .lineLimit(1)
      Spacer(minLength: Spacing.m)
      Button(action: show) {
        HStack(spacing: Spacing.xsPlus) {
          Image(systemName: "eye")
          Text("Show on screen")
        }
      }
      .buttonStyle(.textLink)
      .help("Outline this window on the screen")
      .accessibilityLabel(window.showLabel)
    }
    .padding(.leading, ViewerLayout.windowIndent)
    .padding(.trailing, Spacing.l)
    .frame(minHeight: ViewerLayout.windowRowHeight)
    .overlay(alignment: .top) {
      SeparatorLine()
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    if let window = PreviewStates.sampleApps.first?.windows.last {
      AppWindowRow(window: window) {}
        .frame(width: ViewerLayout.largeShotLargest.width)
        .sampleSurface()
    }
  }
#endif
