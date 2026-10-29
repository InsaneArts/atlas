import SwiftUI
import Testing

@testable import AtlasUI

@Suite("AppCardGrid")
@MainActor
struct AppCardGridTests {
  @Test("a grid holds as many columns as its width fits at 232 each, from one up to four")
  func columnsFollowTheWidth() {
    let grid = AppCardGrid()
    #expect(grid.columns(for: 200) == 1)
    #expect(grid.columns(for: 475) == 1)
    #expect(grid.columns(for: 476) == 2)
    #expect(grid.columns(for: 720) == 3)
    #expect(grid.columns(for: 2000) == 4)
    #expect(grid.columnWidth(for: 476) == 232)
  }

  @Test("every card in a row is as tall as the row's tallest, so a grid never looks ragged")
  func rowsAreEven() throws {
    let pixels = try PanelPixels(
      AppCardGrid {
        Color.red
          .frame(minHeight: 40)
        Color.blue
          .frame(minHeight: 80)
        Color.green
          .frame(minHeight: 20)
      }
      .frame(width: 476)
      .fixedSize(horizontal: false, vertical: true)
      .background(Color.black)
    )
    // Two columns of 232 with a gap of 12: the first row is 80 high, the second starts 12 under it.
    #expect(pixels.height == 80 + 12 + 20)
    #expect(pixels.colour(x: 100, y: 70).red > 0.9, "the shorter card fills its row")
    #expect(pixels.colour(x: 350, y: 70).blue > 0.9)
    #expect(pixels.colour(x: 100, y: 100).green > 0.4)
    #expect(pixels.colour(x: 350, y: 100).blue < 0.1, "nothing beside the last card")
  }

  @Test("a card lists up to three windows, else two and the rest in “N more”")
  func aCardListsAFew() {
    let windows = (1...5)
      .map { PreviewStates.window($0, "Window \($0)", "520 × 860") }
    let three = AppCard.listed(Array(windows.prefix(3)))
    #expect(three.listed.count == 3)
    #expect(three.more.isEmpty)
    let five = AppCard.listed(windows)
    #expect(five.listed.map(\.id) == [1, 2])
    #expect(five.more.map(\.id) == [3, 4, 5])
  }

  @Test("an Apps pane kept once it hides forgets every window's picture and keeps where the windows sit")
  func aKeptPaneForgetsItsPictures() throws {
    let pane = ViewerPane.apps(PreviewStates.sampleAppsPane)
    guard case .apps(let shown) = pane, case .apps(let kept) = pane.forgettingPictures else {
      Issue.record("not Apps")
      return
    }
    #expect(shown.cards.contains { $0.picture?.window != nil }, "the sample cuts Opie's window from the screen")
    #expect(kept.cards.allSatisfy { $0.picture?.window == nil })
    #expect(kept.cards.map(\.picture?.map) == shown.cards.map(\.picture?.map))
    #expect(ViewerPane.screen.forgettingPictures == .screen)
  }

  @Test("a part of a picture is cut from its pixels by shares, and none lies outside it")
  func aPictureIsCutByShares() throws {
    let picture = try #require(SampleDesktop.picture)
    let part = try #require(
      picture.cropped(to: CGRect(x: 0.25, y: 0.5, width: 0.5, height: 0.25), id: "part")
    )
    #expect(part.image.width == picture.image.width / 2)
    #expect(part.image.height == picture.image.height / 4)
    #expect(part.id == "part")
    #expect(picture.cropped(to: CGRect(x: 1.5, y: 0, width: 0.2, height: 0.2), id: "none") == nil)
  }
}
