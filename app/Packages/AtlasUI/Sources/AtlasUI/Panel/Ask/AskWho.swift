import SwiftUI

/// Who asks a question, led by the agent's mark, and at its trailing edge how long the question has waited, in
/// tabular digits, so the line keeps its width as the seconds change.
struct AskWho: View {
  /// The agent's mark.
  let icon: RowIcon
  /// Who asks, such as "Claude Code · release".
  let who: String
  /// How long it has waited, such as "0:42".
  let waited: String

  /// The mark, who asks and the time.
  var body: some View {
    HStack(spacing: AskCardLayout.whoGap) {
      SessionIcon(icon: icon, side: AskCardLayout.whoIcon)
      Text(who)
        .foregroundStyle(Color.textSecondary)
        .lineLimit(1)
        .truncationMode(.tail)
        .frame(maxWidth: .infinity, alignment: .leading)
      Text(waited)
        .monospacedDigit()
        .foregroundStyle(Color.textTertiary)
        .fixedSize()
        .accessibilityHidden(true)
    }
    .font(Typography.font(.callout))
  }
}
