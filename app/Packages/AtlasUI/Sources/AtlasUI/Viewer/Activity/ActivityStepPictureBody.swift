import SwiftUI

/// A step's picture button as ``ActivityStepPictureStyle`` draws it.
struct ActivityStepPictureBody: View {
  /// Its symbol and whether it is pressed.
  let configuration: ButtonStyleConfiguration
  @State private var isHovered = false
  @Environment(\.isFocused) private var isFocused
  @Environment(\.hoverPaused) private var hoverPaused

  /// The symbol in its capsule, centred in its hit area.
  var body: some View {
    let lit = (isHovered && !hoverPaused) || configuration.isPressed
    configuration.label
      .foregroundStyle(lit ? Color.textPrimary : Color.textTertiary)
      .frame(width: ViewerLayout.pictureButton.width, height: ViewerLayout.pictureButton.height)
      .background(
        Capsule()
          .fill(configuration.isPressed ? Color.fillControlHover : Color.fillControl)
          .opacity(lit ? 1 : 0)
          .animation(Motion.hover(isHovered), value: isHovered && !hoverPaused)
          .animation(
            CapsuleButtonStyle.pressAnimation(isPressed: configuration.isPressed),
            value: configuration.isPressed
          )
      )
      .overlay {
        if isFocused {
          Capsule()
            .strokeBorder(Color.strokeFocus, lineWidth: Stroke.thick)
        }
      }
      .frame(width: ViewerLayout.pictureButtonHit, height: ViewerLayout.pictureButtonHit)
      .contentShape(Rectangle())
      .padding(.horizontal, -(ViewerLayout.pictureButtonHit - ViewerLayout.pictureButton.width) / 2)
      .padding(.vertical, -(ViewerLayout.pictureButtonHit - ViewerLayout.pictureButton.height) / 2)
      .focusEffectDisabled()
      .onHover { isHovered = $0 }
  }
}
