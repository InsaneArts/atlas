import SwiftUI

/// A session's or a run's disclosure: a chevron in its column that turns down as the row opens, as the sidebar's
/// Ended heading does; under Reduce Motion it swaps rather than turns.
struct OutlineDisclosure: View {
  /// Whether the row is open.
  let isOpen: Bool
  @Environment(\.motionReduced) private var reduceMotion

  /// The chevron, centred in the disclosure column.
  var body: some View {
    Group {
      if reduceMotion {
        Image(systemName: isOpen ? SymbolName.menu.rawValue : SymbolName.next.rawValue)
      } else {
        Image(systemName: SymbolName.next.rawValue)
          .animation(ActivityMotion.header(reduceMotion: reduceMotion)) { chevron in
            chevron.rotationEffect(.degrees(isOpen ? ViewerLayout.chevronTurn : .zero))
          }
      }
    }
    .font(Typography.font(size: ViewerLayout.headingChevron, weight: .semibold))
    .foregroundStyle(Color.textTertiary)
    .frame(width: ViewerLayout.headingChevron, height: ViewerLayout.headingChevron)
    .frame(width: ViewerLayout.outlineDisclosure)
    .accessibilityHidden(true)
  }
}
