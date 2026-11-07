import SwiftUI

/// The row that ends a long day, such as "46 more sessions on Wednesday ›", 32 high with its words at the sessions'
/// text edge, which shows the rest of that day's sessions.
struct ActivityMoreSessionsRow: View {
  /// The row.
  let more: ViewerMoreSessions
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The row's link.
  var body: some View {
    Button(more.title) {
      withAnimation(ActivityMotion.filter(reduceMotion: reduceMotion)) { send(.showDaySessions(more.day)) }
    }
    .buttonStyle(OutlineLinkStyle())
    .padding(.leading, ViewerLayout.outlineText(level: 0) - ViewerLayout.linkInset)
    .frame(height: ViewerLayout.runRowHeight)
  }
}
