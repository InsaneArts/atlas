#if DEBUG
  import AppKit
  import SwiftUI
  import Testing

  @testable import AtlasUI

  /// Every setup screen measured at its window's size, through AppKit's hosting view, which only this test target
  /// imports: AtlasUI itself never does.
  @MainActor
  @Suite("Onboarding fit")
  struct OnboardingFitTests {
    @Test("every setup screen fits its window with 24 points to spare above the footer, every reserved line counted")
    func everyScreenFits() {
      for preview in PreviewStates.onboarding + Self.longest {
        let state = preview.value
        let window = Self.window(state)
        let room = window.height - Self.footer(state) - OnboardingLayout.spare
        let needed = Self.height(state, width: window.width)
        #expect(needed <= room, "\(preview.name) needs \(needed) points of \(room)")
      }
    }

    /// The states that make screens longest beyond the samples: the account form with its working line, the
    /// longest a problem with a long name makes it.
    private static var longest: [PreviewState<OnboardingState>] {
      var flagged = PreviewStates.onboardingCreateBadName
      if case .screen(.create(var screen)) = flagged.content {
        screen.note = .problem(
          "Account names can only use letters, numbers, dots, dashes and underscores. Try “maria-fernanda-montgomery”."
        )
        flagged.content = .screen(.create(screen))
      }
      return [PreviewState(name: "Create, a long problem", value: flagged)]
    }

    /// The window's size for a state: the side panel's beside System Settings, else the card's.
    ///
    /// - Parameter state: What the window shows.
    /// - Returns: The size.
    private static func window(_ state: OnboardingState) -> CGSize {
      if case .permissionPanel = state.content { return OnboardingLayout.side }
      return OnboardingLayout.card
    }

    /// The footer's height under a state.
    ///
    /// - Parameter state: What the window shows.
    /// - Returns: The height.
    private static func footer(_ state: OnboardingState) -> CGFloat {
      if case .permissionPanel = state.content { return OnboardingLayout.sideFooterHeight }
      return OnboardingLayout.footerHeight
    }

    /// The height a state's content asks for at the window's width: from the window's top to its last piece.
    ///
    /// - Parameters:
    ///   - state: What the window shows.
    ///   - width: The window's width.
    /// - Returns: The height, in points.
    private static func height(_ state: OnboardingState, width: CGFloat) -> CGFloat {
      let content: AnyView =
        switch state.content {
        case .permissionPanel(let panel):
          AnyView(PermissionPanel(state: panel, dragIcon: AppIcon.dragSample))
        case .loading(let problem):
          AnyView(LoadingScreen(problem: problem))
        case .screen(let screen):
          AnyView(
            OnboardingScreenContent(screen: screen, note: state.note, form: .constant(form(state))) { _ in }
          )
        }
      let host = NSHostingView(
        rootView:
          content
          .fixedSize(horizontal: false, vertical: true)
          .frame(width: width)
          .environment(\.animatesArrivals, false)
      )
      return host.fittingSize.height
    }

    /// The form a state draws with: the bad name's for its samples, else a new window's.
    ///
    /// - Parameter state: What the window shows.
    /// - Returns: The form.
    private static func form(_ state: OnboardingState) -> OnboardingAccountForm {
      guard case .screen(.create(let screen)) = state.content, screen.flagged != nil else {
        return PreviewStates.onboardingForm
      }
      return PreviewStates.onboardingFormBadName
    }
  }
#endif
