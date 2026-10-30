import SwiftUI

/// A card's head: the app's icon at 44 points beside its name and the line under it. While every session's apps show,
/// an agent's app says when it opened at the end of its name's row and names the session under it, with the host's
/// icon, so the session's name has the row to itself; any other says when it opened, or since when it has been open,
/// under its name.
struct AppCardHead: View {
  /// The card.
  let card: ViewerAppCard

  /// The icon, the name and the line under it.
  var body: some View {
    HStack(spacing: ViewerLayout.appCardIconGap) {
      AppGlyph(face: card.icon, side: ViewerLayout.appCardIcon)
      VStack(alignment: .leading, spacing: ViewerLayout.appCardNameGap) {
        HStack(alignment: .firstTextBaseline, spacing: Spacing.s) {
          Text(card.name)
            .font(Typography.font(.headline, weight: .semibold))
            .foregroundStyle(Color.textPrimary)
          if card.origin != nil, !card.line.isEmpty {
            Spacer(minLength: .zero)
            Text(card.line)
              .font(Typography.digits(.callout))
              .foregroundStyle(Color.textTertiary)
              .fixedSize()
          }
        }
        line
      }
      .lineLimit(1)
      Spacer(minLength: .zero)
    }
    .accessibilityElement(children: .ignore)
    .accessibilityLabel(card.label)
    .accessibilityAddTraits(.isHeader)
  }

  /// The session that opened it, with its host's icon; or when it opened, or since when it has been open.
  @ViewBuilder private var line: some View {
    if let origin = card.origin {
      HStack(spacing: ViewerLayout.appCardSessionGap) {
        if let icon = origin.icon {
          SessionIcon(icon: icon, side: ViewerLayout.appCardSessionIcon)
        }
        Text(origin.name)
          .font(Typography.font(.callout))
          .truncationMode(.tail)
      }
      .foregroundStyle(Color.textSecondary)
    } else {
      Text(card.line)
        .font(Typography.digits(.callout))
        .foregroundStyle(Color.textSecondary)
    }
  }
}
