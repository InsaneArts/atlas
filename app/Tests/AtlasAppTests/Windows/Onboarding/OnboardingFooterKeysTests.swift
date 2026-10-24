#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI
  import Testing

  /// The setup window's view in a window of its own, as the app shows it, so Return and Escape reach its buttons
  /// through AppKit's key equivalents.
  @MainActor
  @Suite("OnboardingFooter keys")
  struct OnboardingFooterKeysTests {
    @Test("every screen hands exactly one main action to the footer, and Return presses it")
    func returnPressesTheMainAction() throws {
      let window = KeyedWindow()
      for preview in PreviewStates.onboarding {
        let state = preview.value
        window.show(state)
        let pressed = window.press(.return)
        switch state.content {
        case .loading(problem: nil), .permissionPanel:
          #expect(state.footer.next == nil, "\(preview.name)")
          #expect(pressed.isEmpty, "\(preview.name)")
        case .screen(.permissions(let screen)):
          // Each row keeps its own Allow… (Q-D3), and Return allows the next permission, as it always has; nothing
          // while both are being checked.
          #expect(state.footer.next == nil, "\(preview.name)")
          let next = [screen.accessibility, screen.screenRecording].first { $0.canAllow && !$0.isOn }
          #expect(pressed == (next.map { [.openSettings(id: $0.id)] } ?? []), "\(preview.name)")
        case .loading, .screen:
          let next = try #require(state.footer.next, "\(preview.name) has no main action")
          let presses = next.isEnabled && next.isDefault
          #expect(pressed == (presses ? [next.event] : []), "\(preview.name)")
        }
        let back = state.footer.back
        let escapes = back.map { $0.title == .back && $0.isEnabled } ?? false
        #expect(window.press(.escape) == (escapes ? back.map { [$0.event] } ?? [] : []), "\(preview.name): Escape")
      }
    }

    @Test("Return presses the action the footer shows now, even when the screen before had a button of the same title")
    func returnFollowsTheFooter() {
      let window = KeyedWindow()
      var state = PreviewStates.onboardingChoose
      window.show(state)
      #expect(window.press(.return) == [.continueChoice])
      state.footer.next = OnboardingFooterAction(
        title: .continue,
        event: .connectAgents,
        isEnabled: true,
        isDefault: true
      )
      window.show(state)
      #expect(window.press(.return) == [.connectAgents])
    }
  }
#endif
