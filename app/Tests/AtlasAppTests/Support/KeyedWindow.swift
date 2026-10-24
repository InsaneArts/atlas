#if DEBUG
  import AppKit
  import AtlasUI
  import SwiftUI

  /// A window holding the setup window's view, which records what its buttons send.
  @MainActor
  final class KeyedWindow {
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
      if let event = NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: key.modifiers,
        timestamp: 0,
        windowNumber: window.windowNumber,
        context: nil,
        characters: key.characters,
        charactersIgnoringModifiers: key.characters,
        isARepeat: false,
        keyCode: key.code
      ) {
        _ = window.performKeyEquivalent(with: event)
      }
      return sent
    }
  }
#endif
