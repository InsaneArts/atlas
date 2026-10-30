import AppKit
import AtlasService
import AtlasServiceTesting
import AtlasUI
import CoreGraphics
import Testing

@testable import AtlasApp

@Suite("ViewerController+Size", .serialized, AppKitState())
struct ViewerControllerSizeTests {
  /// A window of `width` at the standard height.
  func wide(_ width: CGFloat) -> CGSize {
    CGSize(width: width, height: PreviewStates.viewerSize.height)
  }

  @Test("the sidebar hides as the window narrows under 688 and shows again from 700, as the state draws it")
  func theSidebarHidesWithTheWidth() {
    let viewer = ViewerController.scripted(ScriptedService())
    defer { viewer.close() }
    viewer.fitWindow(to: wide(1200))
    #expect(viewer.state.sidebar.mode == .full)
    viewer.fitWindow(to: wide(690))
    #expect(viewer.state.sidebar.mode == .full, "it holds within the threshold's room")
    viewer.fitWindow(to: wide(687))
    #expect(viewer.state.sidebar.mode == .hidden)
    viewer.fitWindow(to: wide(699))
    #expect(viewer.state.sidebar.mode == .hidden, "it comes back only at 700")
    viewer.fitWindow(to: wide(400))
    #expect(viewer.state.sidebar.mode == .hidden)
    #expect(viewer.state.footer.fit == .compact)
    viewer.fitWindow(to: wide(1200))
    #expect(viewer.state.sidebar.mode == .full)
    #expect(viewer.state.footer.fit == .full)
  }

  @Test("a narrow window hides the sidebar without changing the person's choice")
  func theWidthNeverChangesThePersonsChoice() {
    let preferences = ViewerPreferences.memory()
    let viewer = ViewerController.scripted(ScriptedService(), preferences: preferences)
    defer { viewer.close() }
    viewer.fitWindow(to: wide(400))
    #expect(viewer.state.sidebar.mode == .hidden)
    #expect(!viewer.session.sidebarHidden)
    #expect(!preferences.loadSidebarHidden())
    viewer.fitWindow(to: wide(860))
    #expect(viewer.state.sidebar.mode == .full, "widening brings it back")
    // The person's own hiding holds at every width.
    viewer.send(.toggleSidebar)
    viewer.fitWindow(to: wide(1200))
    #expect(viewer.state.sidebar.mode == .hidden)
    #expect(preferences.loadSidebarHidden())
  }

  @Test("the toggle shows the sidebar over the card while the window is too narrow for it")
  func theToggleShowsTheSidebarOverTheCard() {
    let preferences = ViewerPreferences.memory()
    let viewer = ViewerController.scripted(ScriptedService(), preferences: preferences)
    defer { viewer.close() }
    viewer.fitWindow(to: wide(400))
    viewer.send(.toggleSidebar)
    #expect(viewer.state.sidebar.overlay)
    #expect(viewer.state.sidebar.mode == .hidden, "the card keeps its room under it")
    #expect(!preferences.loadSidebarHidden(), "showing it over the card is not the person hiding or showing it")
    viewer.send(.toggleSidebar)
    #expect(!viewer.state.sidebar.overlay)
    // A choice in it, or Escape or a click on the card, puts it away.
    viewer.send(.toggleSidebar)
    viewer.send(.chooseLibrary(.activity))
    #expect(!viewer.state.sidebar.overlay)
    viewer.send(.toggleSidebar)
    viewer.send(.dismissSidebarOverlay)
    #expect(!viewer.state.sidebar.overlay)
    // Once the window has room for the whole sidebar beside the card, it goes.
    viewer.send(.toggleSidebar)
    viewer.fitWindow(to: wide(1200))
    #expect(!viewer.state.sidebar.overlay)
    #expect(viewer.state.sidebar.mode == .full)
  }

  @Test("Find in a narrow window shows the whole sidebar over the card with its search")
  func findShowsTheSidebarOverTheCard() {
    let viewer = ViewerController.scripted(ScriptedService())
    defer { viewer.close() }
    viewer.fitWindow(to: wide(600))
    viewer.find()
    #expect(viewer.state.sidebar.overlay)
    #expect(viewer.state.sidebar.mode == .hidden, "the card keeps its room under it")
    #expect(viewer.state.sidebar.search.isOpen)
  }

  @Test("Find in a wide window shows the sidebar the person hid beside the card with its search")
  func findShowsTheHiddenSidebarBesideTheCard() {
    let preferences = ViewerPreferences.memory()
    let viewer = ViewerController.scripted(ScriptedService(), preferences: preferences)
    defer { viewer.close() }
    viewer.fitWindow(to: wide(1200))
    viewer.send(.toggleSidebar)
    #expect(viewer.state.sidebar.mode == .hidden)
    viewer.find()
    #expect(viewer.state.sidebar.mode == .full)
    #expect(!viewer.state.sidebar.overlay)
    #expect(viewer.state.sidebar.search.isOpen)
    #expect(!preferences.loadSidebarHidden())
  }

  @Test("the Library folds into a row of icons below 420 high")
  func theLibraryFoldsInAShortWindow() {
    let viewer = ViewerController.scripted(ScriptedService())
    defer { viewer.close() }
    viewer.fitWindow(to: CGSize(width: 1200, height: 300))
    #expect(viewer.state.sidebar.isShort)
    viewer.fitWindow(to: CGSize(width: 1200, height: 425))
    #expect(viewer.state.sidebar.isShort, "it opens again only past its room")
    viewer.fitWindow(to: CGSize(width: 1200, height: 432))
    #expect(!viewer.state.sidebar.isShort)
  }

  @Test("the window has no least size of its own and fits its content to its frame as it is made")
  func theWindowFitsItsContentAsItIsMade() {
    _ = NSApplication.shared
    let viewer = ViewerController.scripted(ScriptedService())
    let window = ViewerWindow(controller: viewer) { _ in }
    defer {
      window.close()
      NSWindow.removeFrame(usingName: ViewerWindow.autosaveName)
    }
    #expect(viewer.session.windowSize == window.frame.size)
    window.setContentSize(NSSize(width: 400, height: 300))
    #expect(window.frame.width == 400)
    #expect(viewer.state.sidebar.mode == .hidden)
    #expect(viewer.state.sidebar.isShort)
  }

  @Test("a restored frame chooses its sidebar before the first draw")
  func aRestoredFrameChoosesItsSidebarFirst() {
    _ = NSApplication.shared
    let name = ViewerWindow.autosaveName
    let saved = NSWindow(
      contentRect: NSRect(x: 100, y: 100, width: 600, height: 500),
      styleMask: [.titled, .resizable],
      backing: .buffered,
      defer: true
    )
    saved.saveFrame(usingName: name)
    defer { NSWindow.removeFrame(usingName: name) }
    let viewer = ViewerController.scripted(ScriptedService())
    let window = ViewerWindow(controller: viewer) { _ in }
    defer { window.close() }
    #expect(window.frame.width == saved.frame.width)
    #expect(viewer.state.sidebar.mode == .hidden, "fitted to the frame it opens at, not the first size")
  }

  @Test("the Screenshots grid keeps its columns while the window is being resized and settles them after")
  func theGridKeepsItsColumnsWhileResizing() async throws {
    let viewer = try await ViewerPanesModelGridTests.onGrid(ScriptedService())
    defer { viewer.close() }
    viewer.setLiveResizing(true)
    viewer.send(.shotColumns(4))
    guard case .screenshots(let held) = viewer.state.pane else {
      Issue.record("Screenshots shows")
      return
    }
    #expect(held.heldColumns == 2)
    #expect(viewer.state.isResizing)
    viewer.setLiveResizing(false)
    guard case .screenshots(let settled) = viewer.state.pane else {
      Issue.record("Screenshots shows")
      return
    }
    #expect(settled.heldColumns == nil)
    #expect(!viewer.state.isResizing)
  }
}
