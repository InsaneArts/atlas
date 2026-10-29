import Testing

@testable import AtlasService

@Suite("HelperWindowVisibility")
struct WindowVisibilityTests {
  /// A 1440 by 900 display.
  static let display = HelperDisplay(width: 1440, height: 900)
  /// A window of 800 by 500 at 100, 100, second from the front.
  static let window = HelperWindow(id: 1, x: 100, y: 100, width: 800, height: 500, order: 1)

  @Test("a window nothing covers shows whole")
  func nothingCoversIt() {
    #expect(HelperWindowVisibility.share(of: Self.window, among: [Self.window], display: Self.display) == 1)
    #expect(HelperWindowVisibility.isInView(Self.window, among: [Self.window], display: Self.display))
  }

  @Test("a window in front covers the share of the grid it lies over, and one behind covers nothing")
  func frontWindowsCover() {
    let front = HelperWindow(id: 2, x: 100, y: 100, width: 400, height: 500, order: 0)
    let behind = HelperWindow(id: 3, x: 100, y: 100, width: 800, height: 500, order: 2)
    let windows = [front, Self.window, behind]
    #expect(HelperWindowVisibility.share(of: Self.window, among: windows, display: Self.display) == 0.5)
    #expect(!HelperWindowVisibility.isInView(Self.window, among: [front, Self.window], display: Self.display))
    #expect(HelperWindowVisibility.share(of: Self.window, among: [behind], display: Self.display) == 1)
  }

  @Test("a sliver covered still counts as in view, past a tenth it does not")
  func aTenthIsTheLimit() {
    let sliver = HelperWindow(id: 2, x: 100, y: 100, width: 50, height: 500, order: 0)
    let strip = HelperWindow(id: 3, x: 100, y: 100, width: 120, height: 500, order: 0)
    #expect(HelperWindowVisibility.isInView(Self.window, among: [sliver], display: Self.display))
    #expect(!HelperWindowVisibility.isInView(Self.window, among: [strip], display: Self.display))
  }

  @Test("the part of a window off the display does not show")
  func offTheDisplay() {
    let half = HelperWindow(id: 1, x: 1040, y: 100, width: 800, height: 500, order: 0)
    #expect(HelperWindowVisibility.share(of: half, among: [], display: Self.display) == 0.5)
  }

  @Test("a window whose place in the order is unknown, or of no size, counts as hidden")
  func unknownCountsAsHidden() {
    let unordered = HelperWindow(id: 1, x: 100, y: 100, width: 800, height: 500)
    let empty = HelperWindow(id: 2, x: 100, y: 100, order: 0)
    #expect(HelperWindowVisibility.share(of: unordered, among: [], display: Self.display) == 0)
    #expect(HelperWindowVisibility.share(of: empty, among: [], display: Self.display) == 0)
  }

  @Test("a window stands as before only in the same place at the same size")
  func standsAsBefore() {
    var moved = Self.window
    moved.x += 1
    #expect(Self.window.standsAsBefore(Self.window))
    #expect(!moved.standsAsBefore(Self.window))
  }
}
