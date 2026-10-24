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

  /// Whether two drawings put the same colour at every point.
  ///
  /// - Parameters:
  ///   - one: A drawing.
  ///   - other: Another, as wide.
  /// - Returns: Whether they match.
  static func same(_ one: PanelPixels, _ other: PanelPixels) -> Bool {
    guard one.width == other.width, one.height == other.height else { return false }
    for y in 0..<one.height {
      for x in 0..<one.width where one.colour(x: x, y: y) != other.colour(x: x, y: y) {
        return false
      }
    }
    return true
  }
}
