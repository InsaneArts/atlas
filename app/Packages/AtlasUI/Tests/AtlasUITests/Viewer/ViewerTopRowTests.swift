import SwiftUI
import Testing

@testable import AtlasUI

@Suite("ViewerTopRow")
@MainActor
struct ViewerTopRowTests {
  @Test("the status line shows in the widest bar alone, and is gone once the tabs show symbols")
  func theStatusLineGoesBeforeTheTabWords() throws {
    let state = PreviewStates.viewerWorking.top
    var bare = state
    bare.status.line = ""
    // 1,440 across holds the widest layout: the line draws.
    #expect(try !Self.same(Self.bar(state, width: 1440), Self.bar(bare, width: 1440)), "the line never drew")
    // 1,040 across takes the tabs' symbols, and the line has gone first, though the status's side has room left.
    #expect(try Self.same(Self.bar(state, width: 1040), Self.bar(bare, width: 1040)), "the line drew at 1,040")
    #expect(try Self.same(Self.bar(state, width: 860), Self.bar(bare, width: 860)), "the line drew at 860")
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
