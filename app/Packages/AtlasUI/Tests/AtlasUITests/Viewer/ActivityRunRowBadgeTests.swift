import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("An Activity run's allowance badge")
struct ActivityRunRowBadgeTests {
  /// A run titled by its site, its steps all allowed for the task.
  static let run = ViewerOutlineRun(
    id: "run.github",
    icon: .symbol(.terminal),
    name: "staging.acme.dev",
    summary: "",
    time: "02:10",
    account: "staging.acme.dev",
    allowance: AllowanceBadge(words: "You allowed for this task", tone: .accent)
  )

  /// How many of a run row's pixels are the badge's blue, drawn at `width`.
  ///
  /// - Parameter width: The row's width, in points.
  /// - Returns: The count.
  /// - Throws: When nothing was drawn.
  private func bluePixels(at width: CGFloat) throws -> Int {
    let pixels = try Pixels(
      ActivityRunRow(run: Self.run, level: 1) { _ in }
        .environment(\.outlineWidth, width)
        .frame(width: width)
    )
    var count = 0
    for y in 0..<pixels.height {
      for x in 0..<pixels.width {
        let colour = pixels.colour(x: x, y: y)
        if colour.alpha > 0.5, colour.blue - colour.red > 0.35 { count += 1 }
      }
    }
    return count
  }

  @Test("the badge shows where the title fits whole beside it, gives way where it doesn't, and VoiceOver keeps it")
  func theBadgeGivesWayToTheTitle() throws {
    #expect(try bluePixels(at: 600) > 0, "room for both")
    #expect(try bluePixels(at: 260) == 0, "the title keeps its room")
    #expect(Self.run.label.contains("You allowed for this task"))
  }
}
