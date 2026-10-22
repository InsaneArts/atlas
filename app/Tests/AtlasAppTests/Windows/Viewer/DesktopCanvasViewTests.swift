import AppKit
import AtlasKit
import Foundation
import Testing

@testable import AtlasApp

@MainActor
@Suite("DesktopCanvasView")
struct DesktopCanvasViewTests {
  /// No range: the desktop ignores where the input method's text would go.
  let none = NSRange(location: NSNotFound, length: 0)

  @Test("composed text shows until committed, then is typed")
  func composedTextIsShownUntilCommittedThenTyped() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    let canvas = DesktopCanvasView()
    canvas.controller = viewer
    canvas.setMarkedText("ni", selectedRange: NSRange(location: 2, length: 0), replacementRange: none)
    #expect(canvas.hasMarkedText())
    #expect(viewer.session.composing == "ni")
    canvas.unmarkText()
    #expect(!canvas.hasMarkedText())
    #expect(viewer.session.composing.isEmpty)
    canvas.setMarkedText("n", selectedRange: NSRange(location: 1, length: 0), replacementRange: none)
    canvas.insertText("你", replacementRange: none)
    #expect(viewer.session.composing.isEmpty)
    try await settle("the committed text was never sent") { service.actions.count == 1 }
    #expect(service.actions.first == [.type(text: "你")])
    viewer.close()
  }

  @Test("the canvas leaving ends a composition under way, and its chip goes once SwiftUI's update has finished")
  func dismantlingEndsTheComposition() async throws {
    let viewer = try await ViewerController.watching(ScriptedService())
    await viewer.takeControl()
    let canvas = DesktopCanvasView()
    canvas.controller = viewer
    canvas.setMarkedText("ni", selectedRange: NSRange(location: 2, length: 0), replacementRange: none)
    DesktopInput.dismantleNSView(canvas, coordinator: ())
    #expect(!canvas.hasMarkedText())
    #expect(viewer.session.composing == "ni", "nothing the window draws changes during the update")
    try await settle("the chip never went") { viewer.session.composing.isEmpty }
    viewer.close()
  }

  @Test("while in control, ⌘2 reaches the helper and the tab stays on Screen")
  func commandTwoReachesTheHelperInControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 200, height: 150),
      styleMask: [],
      backing: .buffered,
      defer: true
    )
    let canvas = DesktopCanvasView(frame: window.contentLayoutRect)
    canvas.controller = viewer
    canvas.takesKeyboard = { _ in true }
    window.contentView = canvas
    try #require(window.makeFirstResponder(canvas), "the screen takes the keyboard in control")
    let commandTwo = try #require(
      NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: .command,
        timestamp: 0,
        windowNumber: window.windowNumber,
        context: nil,
        characters: "2",
        charactersIgnoringModifiers: "2",
        isARepeat: false,
        keyCode: 19
      )
    )
    #expect(canvas.intercept(commandTwo) == nil, "the main menu never sees it")
    try await settle("⌘2 never reached the helper") { service.actions == [[.key("cmd+2")]] }
    #expect(viewer.state.top.tabs.selected == .screen)
    canvas.stopIntercepting()
    viewer.close()
  }

  @Test("at actual size one view point maps to one display point, scrolled or not")
  func actualSizeMapsOnePointToOne() async throws {
    let service = ScriptedService()
    service.display = ViewerStatus.Size(width: 400, height: 300)
    service.picture = png(width: 400, height: 300)
    let viewer = ViewerController.scripted(service)
    viewer.setVisible(true)
    try await settle("the first picture never arrived") { viewer.reading.frame != nil }
    viewer.toggleActualSize()
    let size = try #require(viewer.state.screen.actualSize, "the screen is at actual size")
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 200, height: 150),
      styleMask: [],
      backing: .buffered,
      defer: true
    )
    let scroller = NSScrollView(frame: window.contentLayoutRect)
    let canvas = DesktopCanvasView(frame: NSRect(origin: .zero, size: size))
    scroller.documentView = canvas
    window.contentView = scroller
    for offset in [NSPoint.zero, NSPoint(x: 120, y: 90)] {
      scroller.contentView.scroll(to: offset)
      let target = CGPoint(x: offset.x + 37, y: offset.y + 21)
      let click = try #require(
        NSEvent.mouseEvent(
          with: .leftMouseDown,
          location: canvas.convert(target, to: nil),
          modifierFlags: [],
          timestamp: 0,
          windowNumber: window.windowNumber,
          context: nil,
          eventNumber: 0,
          clickCount: 1,
          pressure: 1
        )
      )
      let point = try #require(viewer.point(canvas.location(click), in: canvas.bounds.size))
      #expect(abs(point.x - target.x) < 0.001 && abs(point.y - target.y) < 0.001, "scrolled to \(offset)")
    }
    viewer.close()
  }

  @Test("while in control, ⌘Q stays with this Mac, and the Keys menu's Quit app sends it to the helper")
  func commandQStaysHereInControl() async throws {
    let service = ScriptedService()
    let viewer = try await ViewerController.watching(service)
    await viewer.takeControl()
    let window = NSWindow(
      contentRect: NSRect(x: 0, y: 0, width: 200, height: 150),
      styleMask: [],
      backing: .buffered,
      defer: true
    )
    let canvas = DesktopCanvasView(frame: window.contentLayoutRect)
    canvas.controller = viewer
    canvas.takesKeyboard = { _ in true }
    window.contentView = canvas
    try #require(window.makeFirstResponder(canvas), "the screen takes the keyboard in control")
    let commandQ = try #require(
      NSEvent.keyEvent(
        with: .keyDown,
        location: .zero,
        modifierFlags: .command,
        timestamp: 0,
        windowNumber: window.windowNumber,
        context: nil,
        characters: "q",
        charactersIgnoringModifiers: "q",
        isARepeat: false,
        keyCode: 12
      )
    )
    #expect(canvas.intercept(commandQ) === commandQ, "this Mac's menu gets it")
    try await Task.sleep(for: Ceiling.absence)
    #expect(service.actions.isEmpty, "nothing went to the helper")
    let quit = try #require(KeysMenu.menu(for: viewer).items.first { $0.title == "Quit app" } as? HandlerMenuItem)
    quit.fire()
    try await settle("Quit app never reached the helper") { service.actions == [[.key("cmd+q")]] }
    canvas.stopIntercepting()
    viewer.close()
  }
}
