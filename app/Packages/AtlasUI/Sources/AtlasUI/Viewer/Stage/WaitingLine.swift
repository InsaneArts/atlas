import SwiftUI

/// The quiet line over the top of a scrolled-down list saying what waits above, such as "3 new sessions", with an up
/// chevron: secondary words in a control-filled capsule on the card's ground that brightens under the pointer. Pressing it takes the list to
/// its top.
struct WaitingLine: View {
  /// What waits.
  let words: String
  /// Takes the list to its top.
  let action: @MainActor () -> Void
  @State private var isHovered = false
  @Environment(\.hoverPaused) private var hoverPaused

  /// The chevron and the words in their capsule.
  var body: some View {
    let lit = isHovered && !hoverPaused
    Button(action: action) {
      HStack(spacing: ViewerLayout.waitingGap) {
        Image(systemName: SymbolName.up.rawValue)
          .font(Typography.font(size: ViewerLayout.waitingChevron, weight: .semibold))
          .accessibilityHidden(true)
        Text(verbatim: words)
          .textStyle(.callout)
          .lineLimit(1)
      }
      .foregroundStyle(Color.textSecondary)
      .padding(.horizontal, ViewerLayout.waitingInset)
      .frame(height: ViewerLayout.waitingHeight)
      .background(
        // On the card's own ground, so the rows scrolling under it never show through its quiet fill.
        Capsule()
          .fill(lit ? Color.fillControlHover : Color.fillControl)
          .animation(Motion.hover(isHovered), value: lit)
          .background(Capsule().fill(Color.fillContent))
      )
      .contentShape(Capsule())
    }
    .buttonStyle(.plain)
    .onHover { isHovered = $0 }
    .accessibilityLabel(Text(verbatim: words))
  }
}
