import SwiftUI
import Testing

@testable import AtlasUI

@Suite("PanelLayout")
struct PanelLayoutTests {
  @Test("the cascade's pieces start 30 milliseconds apart and end within 0.4 seconds")
  func theCascadeIsOneBeat() {
    let pieces = PanelLayout.Piece.allCases
    #expect(pieces == [.word, .line, .picture, .buttons], "the word and its dot, then the line, picture and buttons")
    let starts = pieces.map { PanelLayout.cascadeDelay(index: $0.rawValue) }
    #expect(starts.first == 0, "the word moves at once")
    for (earlier, later) in zip(starts, starts.dropFirst()) {
      #expect(abs(later - earlier - 0.03) < 1e-9)
    }
    let last = starts.last ?? .infinity
    #expect(last + Motion.Springs.state.duration <= 0.4, "the last piece settles within 0.4 seconds")
    #expect(Motion.Duration.state.seconds == Motion.Springs.state.duration, "AppKit's step is the spring's length")
    for piece in pieces {
      #expect(PanelLayout.cascade(piece, reduceMotion: true) == Motion.reduced, "under Reduce Motion: a fade, at once")
      #expect(
        PanelLayout.cascade(piece, reduceMotion: false)
          == Motion.state.delay(PanelLayout.cascadeDelay(index: piece.rawValue))
      )
    }
  }

  @Test("Stop grows in and shrinks away on the slot's own curves, fourth in the cascade with the buttons")
  func stopMovesWithTheButtons() {
    let buttons = PanelLayout.cascadeDelay(index: PanelLayout.Piece.buttons.rawValue)
    #expect(buttons > 0, "after the word, the line and the picture")
    #expect(PanelLayout.slot(isShown: true, reduceMotion: false) == Motion.arrive.delay(buttons))
    #expect(PanelLayout.slot(isShown: false, reduceMotion: false) == Motion.leave.delay(buttons))
    #expect(
      PanelLayout.slot(isShown: true, reduceMotion: true) == Motion.reduced,
      "under Reduce Motion: a fade, at once"
    )
    #expect(PanelLayout.slot(isShown: false, reduceMotion: true) == Motion.reduced)
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

  @Test("the panel's caption chip is v3's small chip, and the working sample's caption fits it whole")
  @MainActor
  func theCaptionFitsItsChip() throws {
    let size = GlassChip<EmptyView>.Size.small
    #expect(size.height == 26 && size.inset == 10 && size.gap == 7)
    let regular = GlassChip<EmptyView>.Size.regular
    #expect(regular.height == 28 && regular.inset == 12 && regular.gap == 8, "the viewer's chip stays as it was")
    let caption = try #require(PreviewStates.panelWorking.picture.caption)
    func width(_ size: GlassChip<Text>.Size) throws -> CGFloat {
      let renderer = ImageRenderer(
        content: GlassChip(lead: .pulsing(.active), time: caption.time, size: size) { Text(caption.text) }
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
