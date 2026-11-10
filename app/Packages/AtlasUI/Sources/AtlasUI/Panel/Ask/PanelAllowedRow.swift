import SwiftUI

/// What the person allowed for a task, while it holds: a check in the success colour, what it allows over the agent
/// and the time left, whose minutes count down in place, and End, on a success-tinted row nested in the panel's card.
struct PanelAllowedRow: View {
  /// The row.
  let row: AllowedRowState
  /// Receives End.
  let send: @MainActor (PanelEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The check, the words and End.
  var body: some View {
    HStack(spacing: AskCardLayout.allowedGap) {
      Image(systemName: SymbolName.passed.rawValue)
        .font(Typography.font(size: IconSize.s))
        .foregroundStyle(Color.success)
        .frame(width: IconSize.s, height: IconSize.s)
        .accessibilityHidden(true)
      VStack(alignment: .leading, spacing: .zero) {
        Text(row.title)
          .font(Typography.font(.callout, weight: .semibold))
          .foregroundStyle(Color.textPrimary)
        Text(row.detail)
          .font(Typography.font(.callout))
          .monospacedDigit()
          .foregroundStyle(Color.textSecondary)
          .contentTransition(Motion.contentTransition(.numericText(countsDown: true), reduceMotion: reduceMotion))
          .animation(Motion.textSwap, value: row.detail)
      }
      .lineLimit(1)
      .truncationMode(.tail)
      .frame(maxWidth: .infinity, alignment: .leading)
      .accessibilityElement(children: .combine)
      Button("End") { send(.endAllowed(row.id)) }
        .buttonStyle(.capsule(.secondary, size: .mini))
        .accessibilityLabel(row.endLabel)
    }
    .padding(AskCardLayout.allowedInset)
    .background(
      RoundedRectangle(cornerRadius: AskCardLayout.corner, style: .continuous)
        .fill(Color.fillSuccessSoft)
    )
  }
}

#if DEBUG
  #Preview("Allowed for a task") {
    PanelView(state: PreviewStates.panelAllowed) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }
#endif
