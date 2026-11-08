import SwiftUI

/// A session's or a run's disclosure: a chevron at its column's leading edge that turns down as the row opens, as the
/// sidebar's Ended heading does; under Reduce Motion it swaps rather than turns.
struct OutlineDisclosure: View {
  /// Whether the row is open.
  let isOpen: Bool
  @Environment(\.motionReduced) private var reduceMotion

  /// The chevron, at the disclosure column's leading edge.
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
    // At the column's leading edge, so its ink stands a point inside the row's content edge, as the day heading's
    // text does.
    .frame(width: ViewerLayout.outlineDisclosure, alignment: .leading)
    .accessibilityHidden(true)
  }
}
