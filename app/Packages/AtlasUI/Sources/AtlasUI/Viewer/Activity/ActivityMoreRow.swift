import SwiftUI

/// "Show N more" under an open moment that holds more steps than it lists, from the steps' words' edge; pressed, the
/// moment grows to list every step it holds, as it grows when it opens.
struct ActivityMoreRow: View {
  /// What it shows.
  let more: ViewerMoreSteps
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  /// The link, at the words' edge.
  var body: some View {
    Button(more.title) {
      withAnimation(ActivityMotion.fold(reduceMotion: reduceMotion)) {
        send(.showMomentSteps(more.moment))
      }
    }
    .buttonStyle(.textLink)
    .fixedSize()
    .frame(minHeight: ViewerLayout.stepHeight)
    .padding(.leading, ViewerLayout.stepLeading)
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}
