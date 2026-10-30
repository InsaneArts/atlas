import SwiftUI
import Testing

@testable import AtlasUI

// On the main actor: a colour that follows the appearance resolves through AppKit, which answers on the main thread.
@Suite("CapsuleButtonStyle")
@MainActor
struct CapsuleButtonStyleTests {
  /// A colour's opacity, resolved in the dark appearance.
  ///
  /// - Parameter color: The colour.
  /// - Returns: Its alpha.
  private func alpha(_ color: Color) -> Float {
    color.resolve(in: .dark).opacity
  }

  @Test("a secondary button's fill steps from 0.08 to 0.12 to 0.16 at rest, on hover and while pressed, in the dark")
  func secondaryFillSteps() {
    let rest = CapsuleButtonStyle.fill(look: .secondary, isHovered: false, isPressed: false, isEnabled: true)
    let hovered = CapsuleButtonStyle.fill(look: .secondary, isHovered: true, isPressed: false, isEnabled: true)
    let pressed = CapsuleButtonStyle.fill(look: .secondary, isHovered: true, isPressed: true, isEnabled: true)
    #expect(abs(alpha(rest) - 0.08) < 0.001)
    #expect(abs(alpha(hovered) - 0.12) < 0.001)
    #expect(abs(alpha(pressed) - 0.16) < 0.001)
    let on = CapsuleButtonStyle.fill(look: .secondary, isOn: true, isHovered: false, isPressed: false, isEnabled: true)
    #expect(on == pressed)
  }

  @Test("a round secondary button takes the held fill while pressed and while what it toggles is on")
  func circlePressAndOnFill() {
    let rest = CapsuleButtonStyle.fill(look: .secondary, isHovered: false, isPressed: false, isEnabled: true)
    let pressed = CapsuleButtonStyle.fill(look: .secondary, isHovered: false, isPressed: true, isEnabled: true)
    let on = CapsuleButtonStyle.fill(look: .secondary, isOn: true, isHovered: false, isPressed: false, isEnabled: true)
    #expect(rest == .fillControl)
    #expect(pressed == .fillControlOn)
    #expect(on == .fillControlOn)
  }

  @Test("a glyph button is unfilled at rest, the secondary fill under the pointer and the held fill while on or held")
  func glyphFillSteps() {
    let rest = CapsuleButtonStyle.fill(look: .glyph, isHovered: false, isPressed: false, isEnabled: true)
    let hovered = CapsuleButtonStyle.fill(look: .glyph, isHovered: true, isPressed: false, isEnabled: true)
    let pressed = CapsuleButtonStyle.fill(look: .glyph, isHovered: true, isPressed: true, isEnabled: true)
    let on = CapsuleButtonStyle.fill(look: .glyph, isOn: true, isHovered: false, isPressed: false, isEnabled: true)
    #expect(rest == .clear)
    #expect(hovered == .fillControl)
    #expect(pressed == .fillControlOn)
    #expect(on == .fillControlOn)
  }

  @Test("a button over a picture is opaque, a step lighter under the pointer and again while held or on, either way")
  func overPictureFillSteps() {
    let rest = CapsuleButtonStyle.fill(look: .overPicture, isHovered: false, isPressed: false, isEnabled: true)
    let hovered = CapsuleButtonStyle.fill(look: .overPicture, isHovered: true, isPressed: false, isEnabled: true)
    let pressed = CapsuleButtonStyle.fill(look: .overPicture, isHovered: true, isPressed: true, isEnabled: true)
    let on = CapsuleButtonStyle.fill(
      look: .overPicture,
      isOn: true,
      isHovered: false,
      isPressed: false,
      isEnabled: true
    )
    #expect(rest == .fillOverPicture)
    #expect(hovered == .fillOverPictureHover)
    #expect(pressed == .fillOverPictureOn)
    #expect(on == .fillOverPictureOn)
    for fill in [rest, hovered, pressed] {
      for environment in [EnvironmentValues.light, .dark] {
        #expect(fill.resolve(in: environment).opacity == 1)
      }
    }
  }

  @Test("a primary capsule is pure white on hover and E3E3E8 while held, in the dark")
  func primaryHoverAndHold() throws {
    let environment = EnvironmentValues.dark
    let hovered = CapsuleButtonStyle.fill(look: .primary, isHovered: true, isPressed: false, isEnabled: true)
      .resolve(in: environment)
    let held = CapsuleButtonStyle.fill(look: .primary, isHovered: true, isPressed: true, isEnabled: true)
      .resolve(in: environment)
    #expect(hovered == Color.white.resolve(in: environment))
    let channels = [held.red, held.green, held.blue]
    for (channel, value) in zip(channels, [Float(0xE3), 0xE3, 0xE8]) {
      #expect(abs(channel * 255 - value) < 0.5)
    }
    let resting = CapsuleButtonStyle.fill(look: .primary, isHovered: false, isPressed: false, isEnabled: true)
    #expect(resting == .textPrimary)
  }

  @Test("a disabled button keeps its fill and dims to the disabled opacity")
  func disabledKeepsItsFill() {
    for look in [CapsuleButtonStyle.Look.primary, .secondary, .overPicture] {
      let resting = CapsuleButtonStyle.fill(look: look, isHovered: false, isPressed: false, isEnabled: true)
      let disabled = CapsuleButtonStyle.fill(look: look, isHovered: true, isPressed: true, isEnabled: false)
      #expect(disabled == resting, "\(look)")
    }
    #expect(CapsuleButtonStyle.opacity(isEnabled: false) == Opacity.disabled)
    #expect(CapsuleButtonStyle.opacity(isEnabled: true) == 1)
  }

  @Test("an alert's button whose command runs stays whole while it waits, and the others dim")
  func aWorkingButtonStaysWhole() {
    #expect(CapsuleButtonStyle.opacity(isEnabled: false, isWorking: true) == 1)
    #expect(CapsuleButtonStyle.opacity(isEnabled: false, isWorking: false) == Opacity.disabled)
  }

  @Test("an alert's accent and quiet buttons brighten under the pointer, the accent to 1A8CFF")
  func alertButtonsBrighten() {
    #expect(CapsuleButtonStyle.fill(look: .accent, isHovered: true, isPressed: false, isEnabled: true) == .accentHover)
    #expect(CapsuleButtonStyle.fill(look: .accent, isHovered: false, isPressed: false, isEnabled: true) == .accent)
    let quiet = CapsuleButtonStyle.fill(look: .tinted, isHovered: true, isPressed: false, isEnabled: true)
    #expect(quiet == .fillControlOn)
    let waiting = CapsuleButtonStyle.fill(look: .destructive, isHovered: true, isPressed: false, isEnabled: false)
    #expect(waiting == .fillControlRaised, "a waiting button keeps its resting fill")
  }
}
