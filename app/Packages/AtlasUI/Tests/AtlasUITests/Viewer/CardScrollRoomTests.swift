import SwiftUI
import Testing

@testable import AtlasUI

@Suite("CardScrollRoom")
struct CardScrollRoomTests {
  /// The small screen in a wide card, 200 by 125, and the band it takes: its height and 12 on each side of it.
  static let pip = CGSize(width: 200, height: 125)

  @Test("the last row of a list can scroll clear of the picture, which floats over the list's whole height")
  func theLastRowCanScrollClearOfThePicture() {
    let band = ViewerLayout.pipRoom(Self.pip)
    #expect(band == 149)
    // At the bottom, the content keeps the band under its last row and nothing else: the list still runs the whole
    // height, so nothing ends early and nothing fades.
    let bottom = CardScrollRoom.contentInsets(MiniClearance(edges: .bottom, length: band))
    #expect(bottom == EdgeInsets(top: 0, leading: 0, bottom: 149, trailing: 0))
    // At the top, the band stands over the first row instead, so the filters and the first row start clear of it.
    let top = CardScrollRoom.contentInsets(MiniClearance(edges: .top, length: band))
    #expect(top == EdgeInsets(top: 149, leading: 0, bottom: 0, trailing: 0))
    // With no small screen there is no room to keep.
    #expect(CardScrollRoom.contentInsets(MiniClearance()) == EdgeInsets())
  }

  @Test("a selected row's room reaches past the picture's band, so a step lands it clear of the picture")
  func aSelectedRowsRoomReachesPastTheBand() {
    let reach = CardScrollRoom.revealReach(MiniClearance(edges: .bottom, length: 149))
    // Its ring's margin all round, and the band at the end the small screen floats over.
    #expect(reach == EdgeInsets(top: 8, leading: 8, bottom: 157, trailing: 8))
    let top = CardScrollRoom.revealReach(MiniClearance(edges: .top, length: 149))
    #expect(top.top == 157 && top.bottom == 8)
    #expect(ViewerLayout.listRevealMargin >= PictureRing.reach, "the ring shows whole too")
    #expect(CardScrollRoom.revealReach(MiniClearance()) == EdgeInsets(top: 8, leading: 8, bottom: 8, trailing: 8))
  }

  @Test("a list holds the row read as rows come in above it, unless it sits at its top")
  func aListHoldsTheRowReadUnlessAtTheTop() {
    // At the top, the list holds no row, so it stays at the top and a new row shows as it comes in.
    #expect(CardScrollRoom.anchor(scrolledTo: nil, isAtTop: true) == nil)
    #expect(CardScrollRoom.anchor(scrolledTo: "38", isAtTop: true) == nil)
    // Scrolled down, it holds the row the person scrolled to, which keeps its place as rows come in above it or the
    // list changes around it.
    #expect(CardScrollRoom.anchor(scrolledTo: "30", isAtTop: false) == "30")
  }
}
