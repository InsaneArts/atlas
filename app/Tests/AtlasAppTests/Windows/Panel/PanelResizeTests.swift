import AppKit
import AtlasUI
import SwiftUI
import Testing

@testable import AtlasApp

@Suite("PanelResize")
struct PanelResizeTests {
  @Test(
    "the panel's height eases over a quarter second while it shows, and lands at once while closed, under Reduce Motion or with easing off"
  )
  func theHeightEasesOnlyWhileShown() {
    let eased = PanelResize.eased(0.25)
    #expect(PanelResize.change(isShown: true, reduceMotion: false, eases: true) == eased)
    #expect(PanelResize.change(isShown: false, reduceMotion: false, eases: true) == .instant, "opening shows it whole")
    #expect(PanelResize.change(isShown: true, reduceMotion: true, eases: true) == .instant, "Reduce Motion")
    #expect(PanelResize.change(isShown: true, reduceMotion: false, eases: false) == .instant, "the fallback")
  }

  @Test("each session row past the first grows the panel")
  func sessionRowsGrowThePanel() throws {
    let height = { (snapshot: PanelSnapshot) in
      NSHostingView(rootView: PanelView(state: PanelPresenter.state(snapshot)) { _ in })
        .fittingSize
        .height
    }
    let one = try SessionReplies.snapshot([SessionReplies.claudeActing])
    let two = try SessionReplies.snapshot([SessionReplies.claudeActing, SessionReplies.cursorIdle])
    let three = try SessionReplies.snapshot([
      SessionReplies.claudeActing, SessionReplies.codexWaiting(), SessionReplies.cursorIdle,
    ])
    #expect(height(two) > height(one), "two session rows stand taller than one")
    #expect(height(three) > height(two))
  }
}
