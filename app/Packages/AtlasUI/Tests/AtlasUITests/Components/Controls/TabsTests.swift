import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Tabs")
@MainActor
struct TabsTests {
  @Test("a tab's accessory never moves its word off centre, so the words sit evenly and the thumb on its tab")
  func aTabsAccessoryNeverMovesItsWord() throws {
    for face in [TabsFace.word, .symbol] {
      let bare = try PanelPixels(Self.track(face: face, dot: nil))
      // Activity carries a dot that is not showing, as it does with nothing unseen: the row draws exactly as bare.
      let hidden = try PanelPixels(Self.track(face: face, dot: 0))
      #expect(hidden.width == bare.width, "\(face)")
      #expect(Self.same(hidden, bare), "\(face): the hidden dot moved something")
      // Showing, the dot hangs after the word, inside the tab's own padding: the row keeps its width.
      let shown = try PanelPixels(Self.track(face: face, dot: 1))
      #expect(shown.width == bare.width, "\(face)")
      #expect(!Self.same(shown, bare), "\(face): the dot never drew")
    }
  }

  /// The four sample tabs in a track, Activity selected, Activity carrying a blue dot at `dot`'s opacity, or nothing.
  ///
  /// - Parameters:
  ///   - face: What each tab shows.
  ///   - dot: The dot's opacity, or nil for no accessory at all.
  /// - Returns: The row on the ground.
  static func track(face: TabsFace, dot: Double?) -> some View {
    Tabs(
      state: CapsuleTabsState(tabs: TabsSamples.tabs, selected: .activity, label: "What you see"),
      face: face
    ) { _ in
    } accessory: { tab in
      if tab == .activity, let dot {
        Circle()
          .fill(Color.accent)
          .frame(width: IconSize.xxs, height: IconSize.xxs)
          .opacity(dot)
      }
    }
    .padding(Spacing.m)
    .background(Color.ground)
  }

  /// 0.2: how far apart a channel may be at one point and still count as the same drawing. Text's edges can render a
  /// shade apart from one drawing to the next on a busy Mac; a word moved by even a point changes its edges by far
  /// more, and the blue dot changes its pixels by more still.
  static let sameWithin = 0.2

  /// Whether two drawings lay everything out in the same place: the same size, and at every point colours within
  /// ``sameWithin`` of each other.
  ///
  /// - Parameters:
  ///   - one: A drawing.
  ///   - other: Another, as wide.
  /// - Returns: Whether they match.
  static func same(_ one: PanelPixels, _ other: PanelPixels) -> Bool {
    guard one.width == other.width, one.height == other.height else { return false }
    for y in 0..<one.height {
      for x in 0..<one.width {
        let left = one.colour(x: x, y: y)
        let right = other.colour(x: x, y: y)
        let apart = max(
          abs(left.red * left.alpha - right.red * right.alpha),
          abs(left.green * left.alpha - right.green * right.alpha),
          abs(left.blue * left.alpha - right.blue * right.alpha),
          abs(left.alpha - right.alpha)
        )
        if apart > sameWithin { return false }
      }
    }
    return true
  }
}
