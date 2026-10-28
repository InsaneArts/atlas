import SwiftUI
import Testing

@testable import AtlasUI

@Suite("ViewerTopRow")
@MainActor
struct ViewerTopRowTests {
  @Test("the status line shows in the widest bar alone, and is gone once the tabs show symbols")
  func theStatusLineGoesBeforeTheTabWords() throws {
    // Off the Screen, where no caption says what the agent is doing, Working carries its line.
    let state = PreviewStates.viewerActivity.top
    var bare = state
    bare.status.line = ""
    // 1,440 across holds the widest layout: the line draws.
    #expect(try !Self.same(Self.bar(state, width: 1440), Self.bar(bare, width: 1440)), "the line never drew")
    // 1,040 across takes the tabs' symbols, and the line has gone first, though the status's side has room left.
    #expect(try Self.same(Self.bar(state, width: 1040), Self.bar(bare, width: 1040)), "the line drew at 1,040")
    #expect(try Self.same(Self.bar(state, width: 860), Self.bar(bare, width: 860)), "the line drew at 860")
  }

  @Test("the sessions picker keeps the bar's height and loses its words first")
  func theSessionsPickerKeepsTheBarsHeight() throws {
    let state = PreviewStates.viewerSessions.top
    var bare = state
    bare.sessions = nil
    #expect(try Self.height(state) == Self.height(bare), "the picker makes the bar no taller")
    var renamed = state
    renamed.sessions?.title = "Another title"
    // 1,440 across draws the picker's words; 1,040, where the tabs show symbols, draws its icons and chevron alone.
    #expect(try !Self.same(Self.bar(state, width: 1440), Self.bar(renamed, width: 1440)), "the words never drew")
    #expect(try Self.same(Self.bar(state, width: 1040), Self.bar(renamed, width: 1040)), "the words drew at 1,040")
    #expect(ViewerLayout.barLayouts.map(\.sessionWords) == [true, true, false, false, false])
  }

  /// The bar's own height, 1,440 across.
  ///
  /// - Parameter state: What the bar shows.
  /// - Returns: Its height in points.
  /// - Throws: When nothing was drawn.
  static func height(_ state: ViewerTopRowState) throws -> Int {
    try PanelPixels(
      ViewerTopRow(state: state) { _ in }
        .frame(width: PreviewStates.viewerSize.width)
        .fixedSize(horizontal: false, vertical: true)
    )
    .height
  }

  /// The bar `width` across, drawn.
  ///
  /// - Parameters:
  ///   - state: What the bar shows.
  ///   - width: Its width.
  /// - Returns: The drawing.
  /// - Throws: When nothing was drawn.
  static func bar(_ state: ViewerTopRowState, width: CGFloat) throws -> PanelPixels {
    try PanelPixels(
      ViewerTopRow(state: state) { _ in }
        .frame(width: width, height: ViewerLayout.topRowHeight)
        .background(Color.ground)
    )
  }

  /// Whether two drawings put the same colour at every point.
  ///
  /// - Parameters:
  ///   - one: A drawing.
  ///   - other: Another.
  /// - Returns: Whether they match.
  static func same(_ one: PanelPixels, _ other: PanelPixels) -> Bool {
    TabsTests.same(one, other)
  }
}
