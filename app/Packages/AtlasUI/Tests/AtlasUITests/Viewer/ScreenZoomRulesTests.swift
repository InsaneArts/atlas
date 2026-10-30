import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("ScreenZoom rules")
@MainActor
struct ScreenZoomRulesTests {
  @Test("every list keeps the screen in its corner, and only a switch between the Screen and a list moves the picture")
  func onlyTheScreenAndTheCornerZoom() throws {
    let screen = PreviewStates.viewerWorking.screen
    try #require(screen.picture != nil)
    #expect(ViewerPlace.allCases.filter(\.keepsScreenInCorner) == [.activity, .screenshots, .apps])
    let moves: [(ViewerPlace, ViewerPlace)] = [
      (.screen, .activity), (.screen, .screenshots), (.screen, .apps), (.activity, .screen), (.screenshots, .screen),
      (.apps, .screen),
    ]
    for (old, new) in moves {
      #expect(ScreenZoom.zooms(from: old, to: new, screen: screen, reduceMotion: false), "\(old) to \(new)")
    }
    // Between two lists the small screen simply stays in its corner.
    for (old, new) in [(ViewerPlace.activity, ViewerPlace.apps), (.screenshots, .activity), (.apps, .screenshots)] {
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
    // Arriving on its own, the small screen fades in from a little smaller, 96% of its size.
    #expect(ScreenZoom.opacity(isArriving: true, isAway: true, zooms: false) == 0)
    #expect(ViewerLayout.miniScale(isComing: true, growsFromStage: false, reduceMotion: false) == 0.96)
    #expect(ViewerLayout.miniScale(isComing: true, growsFromStage: true, reduceMotion: false) == 1)
    // Under Reduce Motion nothing moves: no switch zooms, so the small screen and the stage fade, at their own size.
    for (old, new) in [(ViewerPlace.screen, ViewerPlace.activity), (.apps, .screen)] {
      #expect(!ScreenZoom.zooms(from: old, to: new, screen: screen, reduceMotion: true), "\(old) to \(new)")
    }
    #expect(ViewerLayout.miniScale(isComing: true, growsFromStage: false, reduceMotion: true) == 1)
  }
}
