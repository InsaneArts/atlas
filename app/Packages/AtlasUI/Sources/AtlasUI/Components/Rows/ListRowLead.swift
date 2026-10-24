import SwiftUI

/// What leads a ``ListRow``: a tile, the account picture, a letter disc or a check's mark, each in the width
/// ``ListRow/leadWidth(_:)`` gives it.
struct ListRowLead: View {
  /// What leads the row.
  let lead: ListRow.Lead

  /// The lead, or nothing.
  var body: some View {
    switch lead {
    case .none:
      EmptyView()
    case .tile(let icon):
      SymbolTile(icon: icon, size: .large)
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
    }
  }
}
