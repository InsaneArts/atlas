import SwiftUI

/// What Atlas is, before anything is set up.
struct WelcomeScreen: View {
  /// Receives Get Started.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The icon, what Atlas does, and Get Started.
  var body: some View {
    ScreenColumn {
      AppIcon(size: OnboardingLayout.welcomeIcon)
      Headline(title: OnboardingScreen.welcomeTitle, tail: OnboardingScreen.welcomeTail)
        .padding(.top, Spacing.m)
      Lede(
        text:
          "Atlas runs a hidden second login on this Mac. Your agents open their builds there, click through them and come back with screenshots, while you keep working."
      )
      Button("Get Started") { send(.begin) }
        .buttonStyle(.capsule(size: .large))
        .keyboardShortcut(.defaultAction)
        .padding(.top, Spacing.ml)
      Note(text: "You set this up once, in about five minutes. After that Atlas runs by itself.")
    }
  }
}

#if DEBUG
  #Preview("Welcome") {
    WelcomeScreen { _ in }
      .onboardingSample()
  }
#endif
