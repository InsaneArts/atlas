import SwiftUI

/// A question Atlas asks the person before an agent sends, at the top of the panel's content: who asks and how long
/// it has waited, what it would do, the task, what would be sent, the answers it offers, and what Allow for This Task
/// covers, on a card nested in the panel's.
struct PanelAskCard: View {
  /// The question.
  let ask: AskCardState
  /// Receives the answer.
  let send: @MainActor (PanelEvent) -> Void

  /// Who, what, the well, the answers and the scope note.
  var body: some View {
    VStack(alignment: .leading, spacing: AskCardLayout.gap) {
      AskWho(icon: ask.icon, who: ask.who, waited: ask.waited)
      VStack(alignment: .leading, spacing: AskCardLayout.titleGap) {
        Text(ask.title)
          .textStyle(.heading)
          .foregroundStyle(Color.textPrimary)
          .fixedSize(horizontal: false, vertical: true)
        if let task = ask.task {
          Text(task)
            .textStyle(.callout)
            .foregroundStyle(Color.textSecondary)
            .lineLimit(2)
        }
      }
      .accessibilityElement(children: .combine)
      if let content = ask.body {
        AskWell(content: content)
      }
      AskAnswers(ask: ask) { send(.answerAsk(id: ask.id, choice: $0)) }
      if let scope = ask.scope {
        Text(scope)
          .textStyle(.caption)
          .foregroundStyle(Color.textTertiary)
          .multilineTextAlignment(.center)
          .frame(maxWidth: .infinity)
          .fixedSize(horizontal: false, vertical: true)
      }
    }
    .padding(AskCardLayout.inset)
    .background(
      RoundedRectangle(cornerRadius: AskCardLayout.corner, style: .continuous)
        .fill(Color.fillGroup)
    )
    .accessibilityElement(children: .contain)
    .accessibilityLabel(ask.label)
  }
}

#if DEBUG
  #Preview("A typed message") {
    PanelView(state: PreviewStates.panelAskTyped) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("A named click") {
    PanelView(state: PreviewStates.panelAskNamedClick) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("A click with no name") {
    PanelView(state: PreviewStates.panelAskUnnamedClick) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("A password field") {
    PanelView(state: PreviewStates.panelAskPassword) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }

  #Preview("For this task or Deny") {
    PanelView(state: PreviewStates.panelAskTaskOnly) { _ in }
      .panelCard()
      .padding(Spacing.xxxl)
  }
#endif
