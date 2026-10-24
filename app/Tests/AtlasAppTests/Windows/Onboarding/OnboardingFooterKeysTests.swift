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
        case .screen(.permissions):
          // Each row keeps its own Allow… (Q-D3), and Return allows the next permission, as it always has.
          #expect(state.footer.next == nil, "\(preview.name)")
          #expect(pressed == [.openSettings(id: "screenRecording")], "\(preview.name)")
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

  /// A window holding the setup window's view, which records what its buttons send.
  @MainActor
  private final class KeyedWindow {
    /// A key the tests press.
    enum Key {
      /// Return, which presses the default button.
      case `return`
      /// Escape, which presses the cancel button.
      case escape
    }

    /// The window, never shown on screen.
    private let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: OnboardingLayout.card),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    /// The view's host, kept to give it each new state.
    private let host: NSHostingView<OnboardingView<AppIcon>>
    /// What the buttons sent since the last press.
    private var sent: [OnboardingEvent] = []

    /// Creates the window, showing the loading screen.
    init() {
      host = NSHostingView(
        rootView: OnboardingView(state: PreviewStates.onboardingLoading, form: .constant(.init())) { _ in }
      )
      window.contentView = host
    }

    /// Shows `state`, as the controller's next state reaches the window: one view, given a new state.
    ///
    /// - Parameter state: The window's state.
    func show(_ state: OnboardingState) {
      host.rootView = OnboardingView(state: state, form: .constant(.init())) { [weak self] in self?.sent.append($0) }
      host.layoutSubtreeIfNeeded()
    }

    /// Presses a key, as the person would with the window in front.
    ///
    /// - Parameter key: The key.
    /// - Returns: What the buttons sent.
    func press(_ key: Key) -> [OnboardingEvent] {
      sent = []
      let characters = key == .return ? "\r" : "\u{1b}"
      if let event = NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: [],
        timestamp: 0,
        windowNumber: window.windowNumber,
        context: nil,
        characters: characters,
        charactersIgnoringModifiers: characters,
        isARepeat: false,
        keyCode: key == .return ? 36 : 53
      ) {
        _ = window.performKeyEquivalent(with: event)
      }
      return sent
    }
  }
#endif
