import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("ViewerLayout+Sidebar")
struct ViewerLayoutSidebarTests {
  @Test("the header clears the window buttons and the sidebar toggle while the sidebar is hidden")
  func theHeaderClearsTheWindowButtonsAndTheToggle() {
    let lightsEnd = ViewerLayout.lightsInset + ViewerLayout.lightsWidth
    let toggle = ViewerLayout.toggleOrigin(isFullScreen: false)
    #expect(toggle.x >= lightsEnd, "the toggle stands past the window buttons")
    // The header's words start in the window at the card's inset and the header's own.
    let words = ViewerLayout.cardInset + ViewerLayout.headerLeading(mode: .hidden, isFullScreen: false)
    #expect(words >= toggle.x + ViewerLayout.toggleSide)
    #expect(words == 118)
    // In full screen there are no window buttons, so the toggle takes their place and the words follow it.
    let fullScreen = ViewerLayout.cardInset + ViewerLayout.headerLeading(mode: .hidden, isFullScreen: true)
    #expect(fullScreen == ViewerLayout.lightsInset + ViewerLayout.toggleSide + ViewerLayout.toggleClearance)
    // Beside the sidebar the header keeps its own inset.
    #expect(ViewerLayout.headerLeading(mode: .full, isFullScreen: false) == ViewerLayout.headerInset)
  }

  @Test("the sidebar toggle stands beside the window buttons, or in their place in full screen")
  func theToggleStandsBesideTheWindowButtons() {
    #expect(ViewerLayout.toggleOrigin(isFullScreen: false) == CGPoint(x: 82, y: 12))
    #expect(ViewerLayout.toggleOrigin(isFullScreen: true).x == ViewerLayout.lightsInset)
  }

  @Test("the card meets the sidebar, or keeps its inset from the window's edge while the sidebar is hidden")
  func theCardMeetsTheSidebar() {
    #expect(ViewerLayout.sidebarRoom(.full) + ViewerLayout.cardLeading(.full) == 236)
    #expect(ViewerLayout.sidebarRoom(.hidden) + ViewerLayout.cardLeading(.hidden) == ViewerLayout.cardInset)
  }

  @Test("a row's capsule spans the sidebar inside its sides, its content inside the capsule's insets")
  func aRowSpansTheSidebar() {
    #expect(ViewerLayout.pillWidth == 216)
    #expect(ViewerLayout.sidebarInset + ViewerLayout.sidebarRowInset == 20)
    #expect(ViewerLayout.rowContentWidth == 196)
  }
}
