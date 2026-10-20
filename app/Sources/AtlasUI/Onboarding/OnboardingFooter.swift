import SwiftUI

/// The bar under a setup screen: Back on the left, the dots in the middle, the next step's button on the right.
struct OnboardingFooter: View {
  /// What the bar shows.
  let footer: OnboardingFooterState
  /// Receives the buttons' events.
  let send: @MainActor (OnboardingEvent) -> Void

  /// The dots under the two buttons, with a line above.
  var body: some View {
    ZStack {
      if let dots = footer.dots {
        PageDots(index: dots.index, count: dots.count)
      }
      HStack {
        back
        Spacer()
        next
      }
    }
    .padding(.horizontal, OnboardingLayout.barInset)
    .frame(height: OnboardingLayout.footerHeight)
    .overlay(alignment: .top) {
      SeparatorLine()
    }
  }

  /// Back, when the screen has one.
  @ViewBuilder private var back: some View {
    if let back = footer.back {
      Button(Self.title(back.title)) { send(back.event) }
        .buttonStyle(.capsule(.plain))
        .disabled(!back.isEnabled)
    }
  }

  /// The next step's button, when the screen has one.
  @ViewBuilder private var next: some View {
    if let next = footer.next {
      Button(Self.title(next.title)) { send(next.event) }
        .buttonStyle(.capsule())
        .disabled(!next.isEnabled)
        .keyboardShortcut(next.isDefault ? .defaultAction : nil)
    }
  }

  /// Each button's fixed title.
  ///
  /// - Parameter title: Which button it is.
  /// - Returns: Its title.
  private static func title(_ title: OnboardingFooterAction.Title) -> String {
    switch title {
    case .back: "Back"
    case .continue: "Continue"
    case .createAccount: "Create Account"
    case .done: "Done"
    }
  }
}

#if DEBUG
  #Preview("Create") {
    OnboardingFooter(footer: PreviewStates.onboardingCreate.footer) { _ in }
      .frame(width: OnboardingLayout.card.width)
      .windowSurface()
  }
#endif
