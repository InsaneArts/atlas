import SwiftUI
import Testing

@testable import AtlasUI

@Suite("PanelLayout")
struct PanelLayoutTests {
  @Test("a state change moves every piece of the panel together, easing out over 0.2 seconds with no bounce")
  func aStateChangeMovesTogether() {
    #expect(PanelLayout.change == .easeOut(duration: 0.2))
  }

  @Test("Recent's time rolls its digits only while its unit stays, and crossfades when the words change")
  func theTimeRollsOnlyWithinAUnit() {
    #expect(PanelRecentRow.unit(of: "9 s") == PanelRecentRow.unit(of: "12 s"), "digits to digits: one text")
    #expect(PanelRecentRow.unit(of: "59 s") != PanelRecentRow.unit(of: "1 min"), "a new unit is a new text")
    #expect(PanelRecentRow.unit(of: "now") != PanelRecentRow.unit(of: "1 s"), "words to digits: a new text")
  }

  @Test("the empty picture's mark breathes only while connecting, and holds still beside a line")
  func theMarkBreathesOnlyWhileConnecting() {
    let pause = PanelPause(isPaused: false, label: "", help: "")
    #expect(PanelPictureState(look: .connecting, pause: pause).breathes)
    #expect(!PanelPictureState(look: .empty(message: "The helper is logged out"), pause: pause).breathes)
  }

  @Test("under Reduce Motion the live row's dot only fades as it cools, its place held while the row is live")
  func theLiveDotOnlyFadesUnderReduceMotion() {
    #expect(PanelRecentRow.holdsDot(isLive: true, isCooled: false, reduceMotion: false))
    #expect(!PanelRecentRow.holdsDot(isLive: true, isCooled: true, reduceMotion: false), "the words close the gap")
    #expect(PanelRecentRow.holdsDot(isLive: true, isCooled: false, reduceMotion: true))
    #expect(PanelRecentRow.holdsDot(isLive: true, isCooled: true, reduceMotion: true), "cooling moves nothing")
    for reduceMotion in [false, true] {
      for isCooled in [false, true] {
        #expect(!PanelRecentRow.holdsDot(isLive: false, isCooled: isCooled, reduceMotion: reduceMotion))
      }
    }
  }

  @Test("the panel's caption chip is the small chip, and the working sample's caption fits it whole")
  @MainActor
  func theCaptionFitsItsChip() throws {
    let size = GlassChip<EmptyView>.Size.small
    #expect(size.height == 26 && size.inset == 10 && size.gap == 7)
    let regular = GlassChip<EmptyView>.Size.regular
    #expect(regular.height == 28 && regular.inset == 12 && regular.gap == 8, "the viewer's chip stays as it was")
    let caption = try #require(PreviewStates.panelWorking.picture.caption)
    func width(_ size: GlassChip<Text>.Size) throws -> CGFloat {
      let renderer = ImageRenderer(
        content: GlassChip(lead: .dot(.active), time: caption.time, size: size) { Text(caption.text) }
          .fixedSize()
      )
      renderer.scale = 1
      return CGFloat(try #require(renderer.cgImage, "nothing was drawn").width)
    }
    #expect(try width(.small) <= PanelLayout.captionWidth, "the small chip holds it without cutting it")
    #expect(try width(.regular) > PanelLayout.captionWidth, "the viewer's chip would have cut it")
  }

  @Test("the panel's ground is tinted over the glass, and opaque under Reduce Transparency")
  func theGroundIsTintedOverTheGlass() {
    #expect(PanelLayout.groundOpacity(reduceTransparency: false) == 0.8)
    #expect(PanelLayout.groundOpacity(reduceTransparency: true) == 1)
    let ground = PanelLayout.ground
      .resolve(in: EnvironmentValues())
    #expect(abs(ground.red * 255 - 30) < 0.5 && abs(ground.green * 255 - 30) < 0.5 && abs(ground.blue * 255 - 33) < 0.5)
    #expect(ground.opacity == 1, "the ground itself is opaque; the tint is its opacity over the glass")
  }

  @Test("the connecting mark breathes between 0.3 and 0.7, holds at 0.5 when still, and rests at 0.55 beside a line")
  func theConnectingMarkBreathes() {
    #expect(PanelEmptyPicture.markOpacity(breathes: true, isInhaled: false, isStill: false) == 0.3)
    #expect(PanelEmptyPicture.markOpacity(breathes: true, isInhaled: true, isStill: false) == 0.7)
    #expect(PanelEmptyPicture.markOpacity(breathes: true, isInhaled: true, isStill: true) == 0.5)
    #expect(PanelEmptyPicture.markOpacity(breathes: false, isInhaled: true, isStill: false) == 0.55)
  }
}
