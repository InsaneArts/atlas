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
    // The guide closed while the window was hidden.
    let hidden = OnboardingPlacement.placed(guiding: false, last: true) { _ in false }
    #expect(hidden == true, "nothing moved, so the guide's placing is still the last one done")
    let shown = OnboardingPlacement.placed(guiding: false, last: hidden, place: place)
    #expect(shown == false && placings == [false])
    #expect(OnboardingPlacement.placed(guiding: false, last: shown, place: place) == false)
    #expect(placings == [false], "the same placing again does nothing")
  }

  @Test("the guide panel sits at the screen's left, its gap from the edge, centred up and down")
  func theGuideSitsAtTheLeft() {
    let frame = CGSize(width: 320, height: 500)
    let origin = OnboardingPlacement.origin(guiding: true, frame: frame, visible: visible)
    #expect(origin == CGPoint(x: visible.minX + OnboardingPlacement.sideGap, y: visible.midY - 250))
  }
}
