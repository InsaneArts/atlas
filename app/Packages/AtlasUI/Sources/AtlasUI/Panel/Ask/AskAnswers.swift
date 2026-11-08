import SwiftUI

/// A question's answers in the panel: the main one across the card, the accent unless it is Deny, and under it the
/// others side by side, sharing the row, each a 32-point capsule.
struct AskAnswers: View {
  /// The question.
  let ask: AskCardState
  /// Receives the answer the person gives.
  let answer: @MainActor (AskChoice) -> Void

  /// The main answer over the others.
  var body: some View {
    VStack(spacing: AskCardLayout.answerGap) {
      Button(ask.main.title) { answer(ask.main) }
        .buttonStyle(.capsule(ask.main == .deny ? .secondary : .accent, size: .medium, fill: true))
      if !ask.others.isEmpty {
        HStack(spacing: AskCardLayout.answerGap) {
          ForEach(ask.others, id: \.self) { choice in
            Button(choice.title) { answer(choice) }
              .buttonStyle(.capsule(.secondary, size: .medium, fill: true))
          }
        }
      }
    }
  }
}
