import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("MonogramTile")
@MainActor
struct MonogramTileTests {
  @Test("a monogram is the size of a real icon's body: 824 of its square's 1024 points, centred")
  func sizeOfAnIconsBody() throws {
    let side = 100
    let pixels = try PanelPixels(
      MonogramTile(monogram: AppMonogram(name: "Probe"), side: CGFloat(side))
    )
    #expect(pixels.width == side)
    let middle = side / 2
    let across = 0..<side
    let inked = across.filter { x in
      pixels.colour(x: x, y: middle)
        .alpha > 0.5
    }
    let body = Double(side) * 824 / 1024
    #expect(abs(Double(inked.count) - body) <= 1.5, "\(inked.count)")
    let left = try #require(inked.first)
    let right = side - 1 - (try #require(inked.last))
    #expect(abs(left - right) <= 1, "left \(left), right \(right)")
    // The square's corner is outside the body's rounded corner.
    #expect(pixels.colour(x: left + 1, y: left + 1).alpha < 0.5)
  }

  @Test("the tile is its name's tint, lighter towards its top")
  func tintAndLift() throws {
    let pixels = try PanelPixels(
      MonogramTile(monogram: AppMonogram(letter: "", tint: .blue), side: 100)
    )
    let top = pixels.colour(x: 50, y: 16)
    let bottom = pixels.colour(x: 50, y: 84)
    #expect(top.red > bottom.red)
    #expect(abs(bottom.blue - 0xF6 / 255.0) < 0.03, "\(bottom)")
  }
}
