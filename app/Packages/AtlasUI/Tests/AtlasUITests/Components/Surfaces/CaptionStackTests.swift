import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("CaptionStack")
struct CaptionStackTests {
  /// An agent's actions after `count` steps, oldest first, the newest marked as the status marks it.
  ///
  /// - Parameter count: How many actions.
  /// - Returns: The actions.
  private static func actions(_ count: Int) -> [ActivityEntry] {
    (0..<count)
      .map { ActivityEntry(id: String($0), line: "Clicked in Opie", when: "", isNewest: $0 == count - 1) }
  }

  @Test("the trail shows at most three chips, the newest in front")
  func theTrailShowsAtMostThreeChipsTheNewestInFront() throws {
    let shown = CaptionStack.shown(Self.actions(5))
    #expect(shown.map(\.id) == ["2", "3", "4"], "the three newest, oldest first")
    #expect(shown.map(\.depth) == [2, 1, 0])
    let front = try #require(shown.first { $0.depth == 0 })
    #expect(front.entry.isNewest)
    #expect(CaptionStack.shown(Self.actions(2)).map(\.depth) == [1, 0])
    #expect(CaptionStack.shown([]).isEmpty)
    // The front chip draws whole and in place; each one behind it higher, smaller and fainter.
    typealias Layout = CaptionStack.Layout
    #expect(Layout.offset(depth: 0, fanned: false) == 0)
    #expect(Layout.scale(depth: 0, fanned: false) == 1)
    #expect(Layout.opacity(depth: 0, fanned: false) == 1)
    for depth in 1..<Layout.limit {
      #expect(Layout.offset(depth: depth, fanned: false) == -7 * CGFloat(depth))
      #expect(abs(Layout.scale(depth: depth, fanned: false) - (1 - 0.05 * CGFloat(depth))) < 1e-9)
      #expect(abs(Layout.opacity(depth: depth, fanned: false) - (1 - 0.35 * Double(depth))) < 1e-9)
    }
  }

  @Test("fanning the trail spreads each chip 34 points per step")
  func fanningTheTrailSpreadsEachChip34PointsPerStep() {
    typealias Layout = CaptionStack.Layout
    for depth in 0..<Layout.limit {
      #expect(Layout.offset(depth: depth, fanned: true) == -34 * CGFloat(depth))
      #expect(Layout.scale(depth: depth, fanned: true) == 1, "fanned, every chip draws whole")
      #expect(Layout.opacity(depth: depth, fanned: true) == 1)
    }
    // The pile reaches as high as its fan, so the pointer keeps it open over the oldest chip.
    #expect(Layout.height(count: 3, fanned: true) == CGFloat(28 + 68))
    #expect(Layout.height(count: 3, fanned: false) == CGFloat(28 + 14))
    #expect(Layout.height(count: 0, fanned: true) == 0)
  }

  @Test("under Reduce Motion a new action moves no chip, and the chips only fade")
  func underReduceMotionTheChipsOnlyFade() {
    // Places move on the state spring, or at once under Reduce Motion; faintness fades either way.
    #expect(CaptionStack.moving(reduceMotion: false) == Motion.state)
    #expect(CaptionStack.moving(reduceMotion: true) == nil)
    #expect(CaptionStack.fading(reduceMotion: false) == Motion.state)
    #expect(CaptionStack.fading(reduceMotion: true) == Motion.reduced)
  }

  @Test("the newest chip leads with a steady blue dot, since only the window's status dot breathes")
  func theNewestChipsDotIsSteady() {
    let newest = ActivityEntry(id: "1", line: "Clicked in Opie", when: "", isNewest: true)
    #expect(CaptionStack.lead(newest) == GlassChip<EmptyView>.Lead.dot(.active))
  }

  @Test("the chips behind the front one show only their shells until the pile fans out")
  func olderChipsHideTheirWordsUntilFanned() {
    typealias Layout = CaptionStack.Layout
    #expect(Layout.showsWords(depth: 0, fanned: false))
    for depth in 1..<Layout.limit {
      #expect(!Layout.showsWords(depth: depth, fanned: false), "\(depth)")
      #expect(Layout.showsWords(depth: depth, fanned: true), "\(depth)")
    }
  }

  @Test("a chip stepping behind a newer one drops its words at once, so they never cross the newer chip's")
  func aChipSteppingBackDropsItsWordsAtOnce() {
    #expect(CaptionStack.wording(showsWords: false) == nil)
    #expect(CaptionStack.wording(showsWords: true) == Motion.textSwap)
  }
}
