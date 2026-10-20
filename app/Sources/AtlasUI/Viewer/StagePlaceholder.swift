import SwiftUI

/// No picture yet, or none to show: said as an onboarding screen says it.
struct StagePlaceholder: View {
  /// Why there is no picture.
  let placeholder: ViewerStagePlaceholder
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The words for why, read as one unless they hold a button to reach.
  var body: some View {
    VStack(spacing: Spacing.xl) {
      content
    }
    .padding(ViewerLayout.placeholderInset)
    .accessibilityElement(children: children)
  }

  /// The eyebrow, headline and lede for each reason, with Log In… for a logged-out helper.
  @ViewBuilder private var content: some View {
    switch placeholder {
    case .loggedOut(let text, let canLogIn):
      Eyebrow(text: "Logged out")
      Headline(title: "The helper is", tail: "logged out.", size: .title)
      Lede(text: text)
      if canLogIn {
        Button("Log In…") { send(.logIn) }
          .buttonStyle(.capsule(size: .large))
          .padding(.top, Spacing.xs)
      }
    case .unavailable(let reason):
      Eyebrow(text: "Can't show the helper", tone: .attention)
      Headline(title: "The helper is", tail: "out of sight.", size: .title)
      Lede(text: reason)
        .textSelection(.enabled)
    case .paused:
      Eyebrow(text: "Paused")
      Headline(title: "Picture paused.", tail: "Work in the helper continues.", size: .title)
    case .connecting:
      Spinner(size: IconSize.mark)
      Headline(title: "Connecting", tail: "to the helper.", size: .title)
      Lede(text: "The first picture arrives in a moment.")
    }
  }

  /// How VoiceOver reads it: a logged-out helper's words stay apart, so Log In… can be reached.
  private var children: AccessibilityChildBehavior {
    switch placeholder {
    case .loggedOut: .contain
    case .unavailable, .paused, .connecting: .combine
    }
  }
}

#if DEBUG
  #Preview("Connecting") {
    StagePlaceholder(placeholder: .connecting) { _ in }
      .sampleSurface()
  }

  #Preview("Logged out") {
    StagePlaceholder(placeholder: .loggedOut(text: String("Log in to it again."), canLogIn: true)) { _ in }
      .sampleSurface()
  }
#endif
