import SwiftUI
import Testing

@testable import AtlasUI

@Suite("CapsuleButtonStyle")
struct CapsuleButtonStyleTests {
  /// A colour's opacity, resolved.
  ///
  /// - Parameter color: The colour.
  /// - Returns: Its alpha.
  private func alpha(_ color: Color) -> Float {
    color.resolve(in: EnvironmentValues()).opacity
  }

  @Test("a glass button's fill steps from 0.08 to 0.12 to 0.16 at rest, on hover and while pressed")
  func glassFillSteps() {
    let rest = CapsuleButtonStyle.fill(look: .glass, isHovered: false, isPressed: false, isEnabled: true)
    let hovered = CapsuleButtonStyle.fill(look: .glass, isHovered: true, isPressed: false, isEnabled: true)
    let pressed = CapsuleButtonStyle.fill(look: .glass, isHovered: true, isPressed: true, isEnabled: true)
    #expect(abs(alpha(rest) - 0.08) < 0.001)
    #expect(abs(alpha(hovered) - 0.12) < 0.001)
    #expect(abs(alpha(pressed) - 0.16) < 0.001)
    let on = CapsuleButtonStyle.fill(look: .glass, isOn: true, isHovered: false, isPressed: false, isEnabled: true)
    #expect(on == pressed)
  }

  @Test("a round glass button takes the held fill while pressed and while what it toggles is on")
  func circlePressAndOnFill() {
    let rest = CapsuleButtonStyle.fill(look: .glass, isHovered: false, isPressed: false, isEnabled: true)
    let pressed = CapsuleButtonStyle.fill(look: .glass, isHovered: false, isPressed: true, isEnabled: true)
    let on = CapsuleButtonStyle.fill(look: .glass, isOn: true, isHovered: false, isPressed: false, isEnabled: true)
    #expect(rest == .fillControl)
    #expect(pressed == .fillControlOn)
    #expect(on == .fillControlOn)
  }

  @Test("a primary capsule is pure white on hover and E3E3E8 while held")
  func primaryHoverAndHold() throws {
    let environment = EnvironmentValues()
    let hovered = CapsuleButtonStyle.fill(look: .primary, isHovered: true, isPressed: false, isEnabled: true)
      .resolve(in: environment)
    let held = CapsuleButtonStyle.fill(look: .primary, isHovered: true, isPressed: true, isEnabled: true)
      .resolve(in: environment)
    #expect(hovered == Color.white.resolve(in: environment))
    #expect(held == Color(red: 0xE3 / 255, green: 0xE3 / 255, blue: 0xE8 / 255).resolve(in: environment))
    let resting = CapsuleButtonStyle.fill(look: .primary, isHovered: false, isPressed: false, isEnabled: true)
    #expect(resting == .textPrimary)
  }

  @Test("a disabled button keeps its fill and dims to the disabled opacity")
  func disabledKeepsItsFill() {
    for look in [CapsuleButtonStyle.Look.primary, .glass] {
      let resting = CapsuleButtonStyle.fill(look: look, isHovered: false, isPressed: false, isEnabled: true)
      let disabled = CapsuleButtonStyle.fill(look: look, isHovered: true, isPressed: true, isEnabled: false)
      #expect(disabled == resting, "\(look)")
    }
    #expect(CapsuleButtonStyle.opacity(isEnabled: false) == Opacity.disabled)
    #expect(CapsuleButtonStyle.opacity(isEnabled: true) == 1)
  }
}
