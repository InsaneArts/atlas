import SwiftUI

/// What Atlas is, before anything is set up. Get Started is the footer's.
struct WelcomeScreen: View {
  /// The icon, what Atlas does, and how long setup takes.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(
        title: OnboardingScreen.welcomeTitle,
        tail: OnboardingScreen.welcomeTail,
        lede:
          "Atlas runs a hidden second login on this Mac. Your agents open their builds there, click through them and come back with screenshots, while you keep working."
      ) {
        AppIcon(size: OnboardingLayout.welcomeIcon)
      }
      Note(text: "You set this up once, in about five minutes. After that Atlas runs by itself.")
        .padding(.top, OnboardingLayout.Rhythm.status)
        .pagePiece(.status)
    }
  }
}

#if DEBUG
  #Preview("Welcome") {
    WelcomeScreen()
      .onboardingSample()
  }
#endif
