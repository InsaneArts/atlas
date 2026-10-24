import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("ScreenZoom rules")
@MainActor
struct ScreenZoomRulesTests {
  @Test("only a switch between the Screen and a tab with the small screen moves the picture, while fitted")
  func onlyTheScreenAndTheCornerZoom() throws {
    let screen = PreviewStates.viewerWorking.screen
    try #require(screen.picture != nil)
    #expect(ViewerTab.allCases.filter(\.keepsScreenInCorner) == [.activity, .apps])
    let moves: [(ViewerTab, ViewerTab)] = [
      (.screen, .activity), (.screen, .apps), (.activity, .screen), (.apps, .screen),
    ]
    for (old, new) in moves {
      #expect(ScreenZoom.zooms(from: old, to: new, screen: screen, reduceMotion: false), "\(old) to \(new)")
    }
    // Screenshots has no small screen, and between two tabs with one it simply stays in its corner.
    for (old, new) in [(ViewerTab.screen, ViewerTab.screenshots), (.screenshots, .screen), (.activity, .apps)] {
      #expect(!ScreenZoom.zooms(from: old, to: new, screen: screen, reduceMotion: false), "\(old) to \(new)")
    }
    // At actual size the picture scrolls, so it fades as any pane; with no picture there is none to move.
    var actual = screen
    actual.actualSize = CGSize(width: 1440, height: 900)
    #expect(!ScreenZoom.zooms(from: .screen, to: .activity, screen: actual, reduceMotion: false))
    var empty = screen
    empty.picture = nil
    #expect(!ScreenZoom.zooms(from: .screen, to: .activity, screen: empty, reduceMotion: false))
    // The move rides the zoom's spring; every other switch keeps the panes' own timing.
    #expect(ScreenZoom.animation(zooms: true) == Motion.zoom)
    #expect(ScreenZoom.animation(zooms: false) == nil)
  }

  @Test("the side that arrives as the picture moves comes whole, and under Reduce Motion the corner only crossfades")
  func theArrivingSideComesWholeAndReduceMotionCrossfades() {
    let screen = PreviewStates.viewerWorking.screen
    // Moving, the stage's picture or the small screen arrives whole at the other's place; the one leaving fades.
    #expect(ScreenZoom.opacity(isArriving: true, isAway: true, zooms: true) == 1)
    #expect(ScreenZoom.opacity(isArriving: false, isAway: true, zooms: true) == 0)
    // Arriving on its own, the small screen fades in from a little smaller.
    #expect(ScreenZoom.opacity(isArriving: true, isAway: true, zooms: false) == 0)
    #expect(ViewerLayout.miniScale(isComing: true, growsFromStage: false, reduceMotion: false) == 0.9)
    #expect(ViewerLayout.miniScale(isComing: true, growsFromStage: true, reduceMotion: false) == 1)
    // Under Reduce Motion nothing moves: no switch zooms, so the small screen and the stage fade, at their own size.
    for (old, new) in [(ViewerTab.screen, ViewerTab.activity), (.apps, .screen)] {
      #expect(!ScreenZoom.zooms(from: old, to: new, screen: screen, reduceMotion: true), "\(old) to \(new)")
    }
    #expect(ViewerLayout.miniScale(isComing: true, growsFromStage: false, reduceMotion: true) == 1)
  }
}
