import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("App card pictures")
@MainActor
struct AppCardPictureTests {
  @Test(
    "tall and wide window thumbnails retain both ends in the fixed picture slot",
    arguments: [
      CGSize(width: 40, height: 400), CGSize(width: 400, height: 40),
    ],
    [ColorScheme.light, .dark]
  )
  func theWholeWindowFits(size: CGSize, colorScheme: ColorScheme) throws {
    let isTall = size.height > size.width
    let slot = CGSize(width: 240, height: ViewerLayout.appCardPicture)
    let fit = min(slot.width / size.width, slot.height / size.height)
    let fitted = CGRect(
      x: (slot.width - size.width * fit) / 2,
      y: (slot.height - size.height * fit) / 2,
      width: size.width * fit,
      height: size.height * fit
    )
    let picture = ViewerAppPicture(
      map: ViewerScreenMap(aspect: 1, windows: []),
      window: try Self.stripedWindow(size: size)
    )
    for scale: CGFloat in [1, 2] {
      let drawn = try PanelPixels(
        AppCardPicture(picture: picture)
          .frame(width: slot.width)
          // The control fill is translucent. Composite over a neutral opaque surface so neither white in
          // Dark nor black in Light can be mistaken for the picture, and alpha never amplifies tiny channels.
          .background(Color(white: colorScheme == .light ? 0.9 : 0.1)),
        colorScheme: colorScheme,
        scale: scale
      )
      #expect(drawn.width == Int(slot.width * scale))
      #expect(drawn.height == Int(slot.height * scale))
      var content = CGRect.null
      for y in 0..<drawn.height {
        for x in 0..<drawn.width {
          let color = drawn.colour(x: x, y: y)
          // Only the opaque, colourful fixture is content; the neutral slot and background are not.
          if color.alpha > 0.99 && PanelPixels.chroma(color) > 0.5 {
            content = content.union(CGRect(x: x, y: y, width: 1, height: 1))
          }
        }
      }
      try #require(!content.isNull, "no window drawn in \(colorScheme) at \(scale)x")
      // Pixel coverage can round outwards, and interpolation can soften the boundary by a pixel.
      for (actual, expected) in [
        (content.minX, fitted.minX * scale), (content.maxX, fitted.maxX * scale),
        (content.minY, fitted.minY * scale), (content.maxY, fitted.maxY * scale),
      ] {
        #expect(abs(actual - expected) <= 1, "the window must fit, keep its aspect and stay centred")
      }
      // Stripes run along the long axis, so these points prove both ends and the middle survive. Check
      // channel dominance rather than device-dependent near-primary values, away from interpolated edges.
      for index in 0..<3 {
        let fraction = (CGFloat(index) + 0.5) / 3
        let x = isTall ? fitted.midX : fitted.minX + fitted.width * fraction
        let y = isTall ? fitted.minY + fitted.height * fraction : fitted.midY
        let color = drawn.colour(x: Int(x * scale), y: Int(y * scale))
        let channels = [color.red, color.green, color.blue]
        #expect(color.alpha > 0.99)
        for other in 0..<3 where other != index {
          #expect(channels[index] > channels[other] + 0.5, "stripe \(index) was cut or misplaced")
        }
      }
      // Halfway into each unused side, clear of the slot's rounded corners, there is only the neutral fill.
      let letterboxes =
        isTall
        ? [
          CGPoint(x: fitted.minX / 2, y: slot.height / 2),
          CGPoint(x: (fitted.maxX + slot.width) / 2, y: slot.height / 2),
        ]
        : [
          CGPoint(x: slot.width / 2, y: fitted.minY / 2),
          CGPoint(x: slot.width / 2, y: (fitted.maxY + slot.height) / 2),
        ]
      for point in letterboxes {
        let color = drawn.colour(x: Int(point.x * scale), y: Int(point.y * scale))
        #expect(color.alpha > 0.99)
        #expect(PanelPixels.chroma(color) < 0.02, "unused room keeps the slot's neutral fill")
      }
    }
  }

  /// An opaque sRGB window with red, green and blue thirds along its long axis.
  ///
  /// - Parameter size: The window's pixel size.
  /// - Returns: The striped window.
  /// - Throws: When its bitmap cannot be made.
  private static func stripedWindow(size: CGSize) throws -> DesktopPicture {
    let space = try #require(CGColorSpace(name: CGColorSpace.sRGB))
    let context = try #require(
      CGContext(
        data: nil,
        width: Int(size.width),
        height: Int(size.height),
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: space,
        bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue | CGImageAlphaInfo.premultipliedLast.rawValue
      )
    )
    let isTall = size.height > size.width
    let length = isTall ? size.height : size.width
    for index in 0..<3 {
      context.setFillColor(red: index == 0 ? 1 : 0, green: index == 1 ? 1 : 0, blue: index == 2 ? 1 : 0, alpha: 1)
      // Integer boundaries leave no partially transparent seams in the source.
      let start = floor(length * CGFloat(index) / 3)
      let end = floor(length * CGFloat(index + 1) / 3)
      context.fill(
        isTall
          // Core Graphics draws up from the bottom; the test reads down from the top.
          ? CGRect(x: 0, y: size.height - end, width: size.width, height: end - start)
          : CGRect(x: start, y: 0, width: end - start, height: size.height)
      )
    }
    return DesktopPicture(id: "striped window", image: try #require(context.makeImage()))
  }
}
