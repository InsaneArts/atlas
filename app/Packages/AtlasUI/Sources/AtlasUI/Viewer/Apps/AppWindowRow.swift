import SwiftUI

/// One of an app's windows under its head: its title and size, a faint fill under the pointer, and Show on
/// Screen, which shows only while the pointer is on this row, or the keyboard is on the button, instead of
/// repeating on every row. It reads as one, with Show on Screen as its action.
struct AppWindowRow: View {
  /// The window.
  let window: ViewerWindowRow
  /// Whether the pointer is on this row, which shows Show on Screen.
  let isHovered: Bool
  /// Receives whether the pointer is on the row.
  let hover: @MainActor (Bool) -> Void
  /// Shows the window on the screen.
  let show: @MainActor () -> Void

  /// The title, the size and Show on Screen, revealed under the pointer.
  var body: some View {
    HStack(spacing: Spacing.sPlus) {
      Text(window.title)
        .font(Typography.font(.callout))
        .lineLimit(1)
      Text(window.size)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textTertiary)
        .lineLimit(1)
      Spacer(minLength: Spacing.m)
      Button(action: show) {
        HStack(spacing: Spacing.xs) {
          Image(systemName: SymbolName.show.rawValue)
          Text(verbatim: "Show on Screen")
        }
      }
      .buttonStyle(.capsule(.glass, size: .mini))
      .help("Outline this window on the screen")
      .revealsOnHover(isHovered)
    }
    .padding(.leading, ViewerLayout.windowIndent)
    .padding(.trailing, Spacing.m)
    .frame(minHeight: ViewerLayout.windowRowHeight)
    .background(fill)
    .modifier(WindowRowSeparator())
    .contentShape(Rectangle())
    .onHover { hover($0) }
    .animation(Motion.hover(isHovered), value: isHovered)
    .accessibilityElement(children: .combine)
    .accessibilityAction(named: Text(window.showLabel)) { show() }
  }

  /// The row's fill: faint under the pointer, none otherwise.
  private var fill: Color {
    isHovered ? .fillRowHover : .clear
  }
}

#if DEBUG
  #Preview("Samples") {
    if let window = PreviewStates.sampleApps.first?.windows.last {
      VStack(spacing: .zero) {
        AppWindowRow(window: window, isHovered: true) { _ in
        } show: {
        }
        AppWindowRow(window: window, isHovered: false) { _ in
        } show: {
        }
      }
      .frame(width: ViewerLayout.largeShotLargest.width)
      .sampleSurface()
    }
  }

  #Preview("Interactive") {
    @Previewable @State var isHovered = false
    InteractivePreview {
      Text(verbatim: isHovered ? "Pointer on the row" : "Pointer away")
    } content: {
      if let window = PreviewStates.sampleApps.first?.windows.last {
        AppWindowRow(window: window, isHovered: isHovered) {
          isHovered = $0
        } show: {
        }
        .frame(width: ViewerLayout.largeShotLargest.width)
      }
    }
  }
#endif
