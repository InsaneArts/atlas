import SwiftUI
import Testing

@testable import AtlasUI

// On the main actor: a colour that follows the appearance resolves through AppKit, which answers on the main thread.
@Suite("PanelLayout")
@MainActor
struct PanelLayoutTests {
  @Test("the picture's frame takes the screen's shape, so no black shows at its sides")
  func thePictureFrameTakesTheScreensShape() {
    #expect(PanelLayout.pictureSize(aspect: 1512.0 / 982.0) == CGSize(width: 328, height: 213))
    #expect(PanelLayout.pictureSize(aspect: 16.0 / 10.0) == PanelLayout.picture)
    #expect(PanelLayout.pictureSize(aspect: 21.0 / 9.0) == CGSize(width: 328, height: 185), "past 16:9 letterboxes")
    #expect(PanelLayout.pictureSize(aspect: 1.0) == CGSize(width: 328, height: 246), "past 4:3 letterboxes")
    #expect(PanelLayout.pictureSize(aspect: nil) == PanelLayout.picture, "no screen keeps 16:10")
    #expect(PanelLayout.pictureSize(aspect: .infinity) == PanelLayout.picture)
  }

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
    #expect(
      !PanelRecentRow.holdsDot(isLive: true, isCooled: true, reduceMotion: false),
      "the words' room opens as it cools"
    )
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
    let size = PictureChip<EmptyView>.Size.small
    #expect(size.height == 26 && size.inset == 10 && size.gap == 7)
    let regular = PictureChip<EmptyView>.Size.regular
    #expect(regular.height == 28 && regular.inset == 12 && regular.gap == 8, "the viewer's chip stays as it was")
    let caption = try #require(PreviewStates.panelWorking.picture.caption)
    func width(_ size: PictureChip<Text>.Size) throws -> CGFloat {
      let renderer = ImageRenderer(
        content: PictureChip(lead: .dot(.active), time: caption.time, size: size) { Text(caption.text) }
          .fixedSize()
      )
      renderer.scale = 1
      return CGFloat(try #require(renderer.cgImage, "nothing was drawn").width)
    }
    #expect(try width(.small) <= PanelLayout.captionWidth, "the small chip holds it without cutting it")
    #expect(try width(.regular) > PanelLayout.captionWidth, "the viewer's chip would have cut it")
  }

  @Test("the panel's ground is opaque in either appearance, so nothing of the popover's material shows through")
  func theGroundIsOpaque() {
    for (environment, rgb) in [(EnvironmentValues.dark, [Float(30), 30, 33]), (.light, [0xF6, 0xF6, 0xF8])] {
      let ground = PanelLayout.ground
        .resolve(in: environment)
      for (channel, value) in zip([ground.red, ground.green, ground.blue], rgb) {
        #expect(abs(channel * 255 - value) < 0.5)
      }
      #expect(ground.opacity == 1)
    }
  }

  @Test("the connecting mark breathes between 0.3 and 0.7, holds at 0.5 when still, and rests at 0.55 beside a line")
  func theConnectingMarkBreathes() {
    #expect(PanelEmptyPicture.markOpacity(breathes: true, isInhaled: false, isStill: false) == 0.3)
    #expect(PanelEmptyPicture.markOpacity(breathes: true, isInhaled: true, isStill: false) == 0.7)
    #expect(PanelEmptyPicture.markOpacity(breathes: true, isInhaled: true, isStill: true) == 0.5)
    #expect(PanelEmptyPicture.markOpacity(breathes: false, isInhaled: true, isStill: false) == 0.55)
  }
}
