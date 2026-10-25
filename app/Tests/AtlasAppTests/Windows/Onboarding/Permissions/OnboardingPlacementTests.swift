import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("OnboardingPlacement")
struct OnboardingPlacementTests {
  /// A screen's visible frame, below a menu bar and beside a Dock.
  let visible = CGRect(x: 0, y: 80, width: 1512, height: 900)

  @Test("the card sits in the middle of the screen")
  func theCardIsCentred() {
    let frame = CGSize(width: 600, height: 500)
    let origin = OnboardingPlacement.origin(guiding: false, frame: frame, visible: visible)
    #expect(origin == CGPoint(x: visible.midX - 300, y: visible.midY - 250))
  }

  @Test("a placing the window could not do, hidden, is done the next time it is asked for")
  func aPlacingThatDidNotRunIsDoneLater() {
    var placings: [Bool] = []
    let place = { (guiding: Bool) -> Bool in
      placings.append(guiding)
      return true
    }
    // The panel closed while the window was hidden.
    let hidden = OnboardingPlacement.placed(guiding: false, last: true) { _ in false }
    #expect(hidden == true, "nothing moved, so the panel's placing is still the last one done")
    let shown = OnboardingPlacement.placed(guiding: false, last: hidden, place: place)
    #expect(shown == false && placings == [false])
    #expect(OnboardingPlacement.placed(guiding: false, last: shown, place: place) == false)
    #expect(placings == [false], "the same placing again does nothing")
  }

  @Test(
    "with System Settings showing no window, the panel sits at the screen's left, its gap from the edge, centred up and down"
  )
  func theGuideSitsAtTheLeft() {
    let frame = CGSize(width: 320, height: 500)
    let origin = OnboardingPlacement.origin(guiding: true, frame: frame, visible: visible)
    #expect(origin == CGPoint(x: visible.minX + OnboardingPlacement.sideGap, y: visible.midY - 250))
  }

  @Test("beside System Settings the panel sits at its right, tops aligned, where the screen leaves room")
  func thePanelSitsRightOfSettings() {
    let frame = CGSize(width: 360, height: 500)
    let settings = CGRect(x: 300, y: 280, width: 700, height: 640)
    let origin = OnboardingPlacement.docked(frame: frame, beside: settings, visible: visible)
    #expect(origin == CGPoint(x: settings.maxX + OnboardingPlacement.settingsGap, y: settings.maxY - 500))
  }

  @Test(
    "with no room at its right the panel sits at System Settings' left, and with room at neither side, over its sidebar"
  )
  func thePanelFallsBackLeftThenInside() {
    let frame = CGSize(width: 360, height: 500)
    let atRight = CGRect(x: 800, y: 280, width: 700, height: 640)
    let left = OnboardingPlacement.docked(frame: frame, beside: atRight, visible: visible)
    #expect(left.x == atRight.minX - OnboardingPlacement.settingsGap - 360)
    let wide = CGRect(x: 100, y: 280, width: 1300, height: 640)
    let inside = OnboardingPlacement.docked(frame: frame, beside: wide, visible: visible)
    #expect(inside.x == visible.minX, "over the sidebar, never the list")
  }

  @Test("the panel stays on screen when System Settings' top is near the menu bar or its window is short")
  func thePanelStaysOnScreen() {
    let frame = CGSize(width: 360, height: 500)
    let short = CGRect(x: 300, y: 100, width: 700, height: 300)
    #expect(OnboardingPlacement.docked(frame: frame, beside: short, visible: visible).y == visible.minY)
    let high = CGRect(x: 300, y: 600, width: 700, height: 600)
    #expect(OnboardingPlacement.docked(frame: frame, beside: high, visible: visible).y == visible.maxY - 500)
  }

  @Test("while System Settings opens the panel shrinks around the middle of where the card was")
  func thePanelShrinksInPlace() {
    let card = CGRect(x: 400, y: 230, width: 720, height: 600)
    let origin = OnboardingPlacement.shrunk(frame: CGSize(width: 360, height: 500), from: card, visible: visible)
    #expect(origin == CGPoint(x: card.midX - 180, y: card.midY - 250))
  }
}
