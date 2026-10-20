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
}
