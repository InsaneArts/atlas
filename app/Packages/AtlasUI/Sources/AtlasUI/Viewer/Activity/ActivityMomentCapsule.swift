import SwiftUI

/// A moment's header on its capsule, which keeps whether the pointer is over it.
struct ActivityMomentCapsule: View {
  /// The button's label and press.
  let configuration: ButtonStyleConfiguration
  /// Whether the moment is open.
  let isOpen: Bool
  /// Whether the person can fold or open it; a moment held open shows no capsule.
  let canFold: Bool
  /// Whether hover waits, as through a live resize.
  let hoverPaused: Bool
  /// Whether the Reduce Motion setting is on.
  let reduceMotion: Bool
  /// Whether the pointer is over it.
  @State private var isHovered = false
  /// Whether the moment was open as last drawn, so a change to it is known from the pointer's.
  @State private var wasOpen: Bool

  /// Creates the capsule.
  ///
  /// - Parameters:
  ///   - configuration: The button's label and press.
  ///   - isOpen: Whether the moment is open.
  ///   - canFold: Whether the person can fold or open it.
  ///   - hoverPaused: Whether hover waits.
  ///   - reduceMotion: Whether the Reduce Motion setting is on.
  init(
    configuration: ButtonStyleConfiguration,
    isOpen: Bool,
    canFold: Bool,
    hoverPaused: Bool,
    reduceMotion: Bool
  ) {
    self.configuration = configuration
    self.isOpen = isOpen
    self.canFold = canFold
    self.hoverPaused = hoverPaused
    self.reduceMotion = reduceMotion
    _wasOpen = State(initialValue: isOpen)
  }

  /// The label on its capsule.
  var body: some View {
    configuration.label
      .padding(.horizontal, ViewerLayout.capsuleReach)
      .frame(height: ViewerLayout.momentHeight)
      .background(
        // Only the paint takes the capsule's fades, so the capsule moves with its header as the list moves it.
        Capsule()
          .animation(fillMotion) { capsule in
            capsule.foregroundStyle(fill)
          }
      )
      .contentShape(Capsule())
      .onHover { isHovered = $0 }
      .onChange(of: isOpen) { _, isOpen in
        wasOpen = isOpen
      }
  }

  /// How the paint changes: as the moment opens or folds, on the spring its chevron turns and its steps grow on;
  /// else at once as it is pressed, and easing as the pointer comes or goes.
  private var fillMotion: Animation? {
    if isOpen != wasOpen { return ActivityMotion.header(reduceMotion: reduceMotion) }
    return configuration.isPressed ? nil : Motion.hover(isHovered)
  }

  /// The capsule's paint: at rest, under the pointer and held, quieter while open; none while the moment is held open.
  private var fill: Color {
    guard canFold else { return .clear }
    let hovered = isHovered && !hoverPaused
    switch (isOpen, configuration.isPressed, hovered) {
    case (_, true, _): return isOpen ? .fillControl : .fillControlOn
    case (false, false, true): return .fillControl
    case (false, false, false): return .fillRowHover
    case (true, false, true): return .fillRowHover
    case (true, false, false): return .clear
    }
  }
}
