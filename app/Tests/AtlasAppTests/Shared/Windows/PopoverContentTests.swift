import AppKit
import Testing

@testable import AtlasApp

@MainActor
@Suite("PopoverContent")
struct PopoverContentTests {
  @Test(
    "a popover's content hands its owner the size its view asks for, first and after each change, and holds the view at its top"
  )
  func theOwnerHearsEachSize() throws {
    let view = NSViewController()
    view.view = NSView(frame: NSRect(x: 0, y: 0, width: 352, height: 100))
    view.preferredContentSize = NSSize(width: 352, height: 100)
    var asked: [NSSize] = []
    let content = PopoverContent(content: view, label: "Atlas") { asked.append($0) }
    content.view.layoutSubtreeIfNeeded()
    #expect(asked == [NSSize(width: 352, height: 100)], "the first size, before the popover shows")
    #expect(content.view.frame.size == NSSize(width: 352, height: 100), "the popover's first size is the view's")
    content.view.needsLayout = true
    content.view.layoutSubtreeIfNeeded()
    #expect(asked.count == 1, "a layout that changes nothing hands on nothing")
    view.preferredContentSize = NSSize(width: 352, height: 160)
    // A hosting view's new size lays the container out, as this does.
    content.view.needsLayout = true
    content.view.layoutSubtreeIfNeeded()
    #expect(asked.last == NSSize(width: 352, height: 160), "a new size goes to the owner, who decides how it lands")
    #expect(asked.count == 2)
    #expect(view.parent === content)
  }
}
