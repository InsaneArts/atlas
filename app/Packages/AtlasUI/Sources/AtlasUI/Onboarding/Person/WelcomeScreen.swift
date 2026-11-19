import SwiftUI

/// What Atlas is, before anything is set up, under the real icon. Get Started is the footer's.
struct WelcomeScreen: View {
  /// The icon, what Atlas does, and how long setup takes.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(
        title: OnboardingScreen.welcomeTitle,
        tail: OnboardingScreen.welcomeTail,
        lede:
          "Your agents open their builds on a desktop of their own, click through them and come back with screenshots, while you keep working."
      ) {
        AppIcon(tile: OnboardingLayout.welcomeTile)
      }
      Note(text: "You set this up once, in about five minutes. After that Atlas runs by itself.")
        .padding(.top, OnboardingLayout.Rhythm.status)
    }
  }
}

#if DEBUG
  #Preview("Welcome") {
    WelcomeScreen()
      .onboardingSample()
  }
#endif
