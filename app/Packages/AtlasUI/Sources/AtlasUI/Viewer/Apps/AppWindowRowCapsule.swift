import SwiftUI

/// A window's row on its capsule, which keeps whether the pointer is over it and shows "Show" at its end while it is.
struct AppWindowRowCapsule: View {
  /// The button's label and press.
  let configuration: ButtonStyleConfiguration
  /// Whether it is the front window of the app in front.
  let isFront: Bool
  /// Whether "Show" shows at its end under the pointer.
  let offersShow: Bool
  /// Whether hover waits, as through a live resize.
  let hoverPaused: Bool
  /// Whether the pointer is over it.
  @State private var isHovered = false

  /// The label and "Show" on the capsule.
  var body: some View {
    HStack(spacing: ViewerLayout.appCardWindowGlyphGap) {
      configuration.label
      Spacer(minLength: .zero)
      if offersShow {
        Text(verbatim: "Show")
          .font(Typography.font(.callout, weight: .semibold))
          .foregroundStyle(Color.textLink)
          .fixedSize()
          .revealsOnHover(isLit)
          .accessibilityHidden(true)
      }
    }
    .padding(.horizontal, ViewerLayout.appCardWindowInset)
    .frame(height: ViewerLayout.appCardWindowHeight)
    .background(
      Capsule()
        .fill(fill)
    )
    .contentShape(Capsule())
    .onHover { isHovered = $0 }
    .animation(Motion.hover(isLit), value: isLit)
    .animation(CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed), value: configuration.isPressed)
  }

  /// Whether the pointer's fill and "Show" show: under the pointer, unless the window is being resized.
  private var isLit: Bool {
    isHovered && !hoverPaused
  }

  /// The capsule's paint: at rest, under the pointer and held, a step up for the front window.
  private var fill: Color {
    switch (configuration.isPressed, isLit, isFront) {
    case (true, _, _): .fillControlOn
    case (false, true, true): .fillControlHover
    case (false, true, false): .fillRowHover
    case (false, false, true): .fillControl
    case (false, false, false): .clear
    }
  }
}
