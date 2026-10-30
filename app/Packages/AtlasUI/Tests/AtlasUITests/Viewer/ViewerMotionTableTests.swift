import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Viewer motion table")
struct ViewerMotionTableTests {
  @MainActor
  @Test("each transition takes the animation the spec names, and a fade of at most 0.2 seconds under Reduce Motion")
  func eachTransitionTakesItsAnimation() {
    let table: [(name: String, moving: Animation?, named: Animation, reduced: Animation?)] = [
      (
        "sidebar", ViewerLayout.sidebarMotion(reduceMotion: false), Motion.slide,
        ViewerLayout.sidebarMotion(reduceMotion: true)
      ),
      (
        "rows", SidebarSessionList.layoutMotion(reduceMotion: false), Motion.state,
        SidebarSessionList.layoutMotion(reduceMotion: true)
      ),
      (
        "Ended", ViewerLayout.endedMotion(reduceMotion: false), Motion.state,
        ViewerLayout.endedMotion(reduceMotion: true)
      ),
      (
        "badge in", StateBadge.appearance(isShown: true, reduceMotion: false), Motion.arrive,
        StateBadge.appearance(isShown: true, reduceMotion: true)
      ),
      (
        "badge out", StateBadge.appearance(isShown: false, reduceMotion: false), Motion.leave,
        StateBadge.appearance(isShown: false, reduceMotion: true)
      ),
      (
        "footer fold", ViewerLayout.footerFoldMotion(reduceMotion: false), Motion.state,
        ViewerLayout.footerFoldMotion(reduceMotion: true)
      ),
      (
        "footer fill", ViewerMainButton.fillMotion(reduceMotion: false), Motion.crossfade,
        ViewerMainButton.fillMotion(reduceMotion: true)
      ),
      (
        "line out", LineSwap<String, String, Text>.fade(isShowing: false), Motion.swapOut,
        LineSwap<String, String, Text>.fade(isShowing: false)
      ),
      (
        "line in", LineSwap<String, String, Text>.fade(isShowing: true), Motion.swapIn,
        LineSwap<String, String, Text>.fade(isShowing: true)
      ),
      ("list in", PaneShift.arrival, Motion.swapIn, PaneShift.arrival),
      ("list out", PaneShift.departure, Motion.swapOut, PaneShift.departure),
      ("picture zoom", ScreenZoom.animation(zooms: true), Motion.zoom, ScreenZoom.animation(zooms: false)),
      ("list rise", ListRise.motion, Motion.zoom, nil),
      (
        "moment opening and folding", ActivityMotion.fold(reduceMotion: false), Motion.state,
        ActivityMotion.fold(reduceMotion: true)
      ),
      (
        "Activity filter", ActivityMotion.filter(reduceMotion: false), Motion.listMove,
        ActivityMotion.filter(reduceMotion: true)
      ),
      (
        "moment header", ActivityMotion.header(reduceMotion: false), Motion.state,
        ActivityMotion.header(reduceMotion: true)
      ),
      (
        "new step", ActivityScroll.moving(isAtTop: true, reduceMotion: false), Motion.state,
        ActivityScroll.moving(isAtTop: true, reduceMotion: true)
      ),
      ("step disc", ActivityMotion.disc, Motion.crossfade, nil),
      (
        "app cards", AppsMotion.layout(reduceMotion: false), Motion.listMove,
        AppsMotion.layout(reduceMotion: true)
      ),
      (
        "app in front", AppsMotion.front(reduceMotion: false), Motion.state,
        AppsMotion.front(reduceMotion: true)
      ),
      ("app picture", AppsMotion.picture, Motion.fade, nil),
    ]
    let fades = [Motion.reduced, Motion.crossfadeReduced, Motion.swapIn, Motion.swapOut]
    for row in table {
      #expect(row.moving == row.named, "\(row.name) moves on the animation the spec names")
      // Under Reduce Motion a transition fades, within 0.2 seconds, or changes at once.
      if let reduced = row.reduced {
        #expect(fades.contains(reduced), "\(row.name) only fades under Reduce Motion")
      }
    }
    // Under Reduce Motion the picture does not travel and no list rises, so both change at once.
    #expect(!ListRise.rises(from: .screen, to: .activity, reduceMotion: true))
    let screen = PreviewStates.viewerWorking.screen
    #expect(ScreenZoom.zooms(from: .screen, to: .activity, screen: screen, reduceMotion: false))
    #expect(!ScreenZoom.zooms(from: .screen, to: .activity, screen: screen, reduceMotion: true))
  }

  @Test("Activity and Apps take nothing from where they move and under Reduce Motion only fade")
  func listsOnlyFadeUnderReduceMotion() {
    // Under Reduce Motion the rows and cards make room at once, as a new step's rows and an opening moment's height
    // do, and fade over 0.15 seconds.
    #expect(AppsMotion.layout(reduceMotion: true) == nil)
    #expect(ActivityMotion.fold(reduceMotion: true) == nil)
    #expect(ActivityScroll.moving(isAtTop: true, reduceMotion: true) == nil)
    // A colour change, the disc's or the picture's, is no movement, so Reduce Motion keeps it, and it is short.
    #expect(ActivityMotion.disc == Animation.easeOut(duration: 0.2))
    #expect(AppsMotion.picture == Animation.easeOut(duration: 0.2))
  }

  @Test("a row leaving a list fades before any row moves over it, and one arriving fades in once the room is made")
  func listChangesNeverOverlap() {
    // Out, then move, then in: the rows that stay wait out the fade of those leaving, so nothing moves over words
    // still drawn, and a row arriving waits until a move that began at once or after that fade has gone nine tenths of
    // the way, so it never fades in under a row still passing.
    #expect(Motion.swapOut == Animation.easeIn(duration: 0.1))
    #expect(Motion.listMove == Animation.spring(Motion.Springs.state).delay(0.1))
    #expect(Motion.listIn == Animation.easeOut(duration: 0.15).delay(0.1 + 0.2))
    let made = Motion.Duration.makeRoom.seconds
    #expect(Motion.Springs.state.value(target: 1.0, time: made) >= 0.9)
    // Words that swap in place wait for the last ones too.
    #expect(Motion.swapInAfterOut == Animation.easeOut(duration: 0.15).delay(0.1))
  }

  @Test("every fade kept under Reduce Motion lasts at most 0.2 seconds")
  func everyKeptFadeIsShort() {
    #expect(Motion.reduced == Animation.easeOut(duration: 0.2))
    #expect(Motion.crossfadeReduced == Animation.easeOut(duration: 0.15))
    #expect(Motion.swapIn == Animation.easeOut(duration: 0.15))
    #expect(Motion.swapOut == Animation.easeIn(duration: 0.1))
  }
}
