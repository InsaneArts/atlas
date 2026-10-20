import SwiftUI

/// Shown while the helper is logged out, in place of its desktop: logging in to it is the way back.
struct LogInCard: View {
  /// What logging in involves.
  let detail: String
  /// What Log In… does.
  let logIn: @MainActor () -> Void

  /// The title and detail over Log In…, on a card.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.ml) {
      TitleDetail(title: "The helper is logged out", detail: detail, size: .compact, wraps: true)
      Button("Log In…", action: logIn)
        .buttonStyle(.capsule(fill: true))
    }
    .padding(Spacing.l)
    .cardSurface()
  }
}

#if DEBUG
  #Preview("Logged out") {
    LogInCard(detail: String("Log in to it once on this Mac, then switch back.")) {}
      .frame(width: SampleWidth.narrow)
      .sampleSurface()
  }
#endif
