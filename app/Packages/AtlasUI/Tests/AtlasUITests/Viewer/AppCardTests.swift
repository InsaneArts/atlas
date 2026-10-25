import SwiftUI
import Testing

@testable import AtlasUI

@Suite("AppCard")
@MainActor
struct AppCardTests {
  /// An app with three windows.
  static let app = ViewerAppRow(
    id: 4,
    name: "Opie",
    windows: [1, 2, 3].map { ViewerWindowRow(id: $0, title: "Window \($0)", size: "520 × 860", showLabel: "Show") }
  )

  @Test("Show on Screen shows only on the hovered window row")
  func showOnScreenShowsOnlyOnTheHoveredRow() throws {
    let pixels = try PanelPixels(
      AppCard(app: Self.app, hoveredWindow: 2) { _ in }
        .frame(width: 600)
        .fixedSize(horizontal: false, vertical: true)
        .background(Color.ground)
    )
    // The head is 46 high and each window's row 34; Show on Screen sits at each row's end.
    let rows = [1, 2, 3].map { (window: $0, middle: 46 + 34 * ($0 - 1) + 17) }
    for row in rows {
      let brightest =
        (pixels.width - 130..<pixels.width - 20)
        .map { x in
          let colour = pixels.colour(x: x, y: row.middle)
          return max(colour.red, colour.green, colour.blue)
        }
        .max() ?? 0
      // The button's white words show on the hovered row alone.
      #expect((brightest > 0.7) == (row.window == 2), "window \(row.window): \(brightest)")
    }
  }
}
