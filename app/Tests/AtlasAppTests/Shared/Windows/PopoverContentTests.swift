import AppKit
import SwiftUI
import Testing

@testable import AtlasApp

@MainActor
@Suite("PopoverContent")
struct PopoverContentTests {
  @Test(
    "a popover's content hands its owner each height its SwiftUI view asks for as it grows and shrinks, holding the view at its top"
  )
  func theOwnerHearsEachSize() throws {
    let height = ViewHeight()
    let popover = NSPopover()
    var asked: [NSSize] = []
    // The shell's own popover path, as the panel's: only the view's state changes; the owner sizes the container.
    WindowShell.host(
      in: popover,
      label: "Atlas",
      follow: { asked.append($0) },
      content: {
        Color.clear
          .frame(width: 352, height: height.value)
      }
    )
    let container = try #require(popover.contentViewController?.view)
    container.layoutSubtreeIfNeeded()
    #expect(asked == [NSSize(width: 352, height: 100)], "the first size, before the popover shows")
    #expect(container.frame.size == NSSize(width: 352, height: 100), "the popover's first size is the view's")
    let child = try #require(container.subviews.first)
    for next in [CGFloat(160), 60] {
      let before = asked.count
      height.value = next
      container.layoutSubtreeIfNeeded()
      #expect(asked.count == before + 1, "\(next): the new size reaches the owner once")
      #expect(asked.last == NSSize(width: 352, height: next), "\(next)")
      // Before the owner's resize lands, the view keeps its own height at the container's top.
      #expect(child.frame.height == next, "\(next)")
      #expect(child.frame.maxY == container.bounds.maxY, "\(next): the view hangs from the top")
      container.setFrameSize(NSSize(width: 352, height: next))
      container.layoutSubtreeIfNeeded()
      #expect(child.frame == container.bounds, "\(next): once landed, the view fills it")
      #expect(asked.count == before + 1, "\(next): a layout that changes nothing hands on nothing")
    }
  }
}

/// The height a test's SwiftUI view draws at, which the test changes as a panel's state would.
@Observable
@MainActor
private final class ViewHeight {
  /// The height, in points.
  var value: CGFloat = 100
}
