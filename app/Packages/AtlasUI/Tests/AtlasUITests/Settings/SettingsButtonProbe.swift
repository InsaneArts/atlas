import AppKit

/// A native action receiver for checking input ownership while a SwiftUI surface fades.
@MainActor
final class SettingsButtonProbe: NSButton {
  /// Actions delivered to this control.
  private(set) var presses = 0

  /// Creates a button with a real target and action.
  init() {
    super.init(frame: .zero)
    title = "Run"
    target = self
    action = #selector(pressed)
  }

  /// Storyboards do not construct this test control.
  required init?(coder: NSCoder) { nil }

  /// Records an action dispatched by AppKit.
  @objc private func pressed() { presses += 1 }
}
