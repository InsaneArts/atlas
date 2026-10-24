import SwiftUI

/// What leads a ``ListRow``: a tile, an app's icon, the account picture, a letter disc, a check's mark or the empty
/// ring of a check not run yet, each in the width ``ListRow/leadWidth(_:)`` gives it. A check's mark is replaced as
/// its state changes, popping in where its owner animates the change (v3 G5).
struct ListRowLead: View {
  /// What leads the row.
  let lead: ListRow.Lead
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The lead, or nothing.
  var body: some View {
    switch lead {
    case .none:
      EmptyView()
    case .tile(let icon):
      SymbolTile(icon: icon, size: .large)
    case .image(let icon):
      Image(decorative: icon.image, scale: 1)
        .resizable()
        .interpolation(.high)
        .frame(width: ListRow.leadWidth(lead), height: ListRow.leadWidth(lead))
    case .picture:
      AccountPicture(side: ListRow.leadWidth(lead))
    case .letter(let letter):
      Text(letter)
        .font(Typography.font(.body, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
        .frame(width: ListRow.leadWidth(lead), height: ListRow.leadWidth(lead))
        .background(
          Circle()
            .fill(Color.fillAvatar)
        )
        .accessibilityHidden(true)
    case .check(let state):
      OutcomeMark(kind: StatusLine.Kind(state), side: ListRow.leadWidth(lead))
        .id(state)
        .transition(
          Motion.transition(.scale(scale: Layout.markArrival).combined(with: .opacity), reduceMotion: reduceMotion)
        )
    case .pending:
      Circle()
        .strokeBorder(Color.strokeControl, lineWidth: Layout.ring)
        .frame(width: ListRow.leadWidth(lead), height: ListRow.leadWidth(lead))
        .accessibilityHidden(true)
    }
  }
}

/// A lead's own measures (v3 `.ring0`, G5).
private enum Layout {
  /// 1.2: the empty ring's line.
  static let ring: CGFloat = 1.2
  /// 0.5: how small a check's new mark starts as it pops in.
  static let markArrival: CGFloat = 0.5
}
