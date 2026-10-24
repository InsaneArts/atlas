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
