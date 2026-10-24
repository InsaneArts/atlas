import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@MainActor
@Suite("PanelPicture")
struct PanelPictureTests {
  /// The room drawn round the picture, so its rim and shadow fall inside the drawing.
  private static let margin = Int(Spacing.l)

  /// A screen of one strong colour, so a change of colour or brightness reads plainly.
  ///
  /// - Returns: The picture.
  /// - Throws: When the picture cannot be made.
  private static func screen() throws -> DesktopPicture {
    let context = try #require(
      CGContext(
        data: nil,
        width: 160,
        height: 100,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: CGColorSpaceCreateDeviceRGB(),
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
      )
    )
    context.setFillColor(red: 0.9, green: 0.25, blue: 0.2, alpha: 1)
    context.fill(CGRect(x: 0, y: 0, width: 160, height: 100))
    return DesktopPicture(id: "red", image: try #require(context.makeImage()))
  }

  /// The picture in a look, drawn with its margin.
  ///
  /// - Parameter look: How the picture looks.
  /// - Returns: Its pixels.
  /// - Throws: When nothing was drawn.
  private static func draw(_ look: PanelPictureState.Look) throws -> PanelPixels {
    let state = PanelPictureState(
      look: look,
      picture: try screen(),
      pause: PanelPause(isPaused: look == .paused, label: "Pause picture", help: "")
    )
    return try PanelPixels(
      PanelPicture(state: state) { _ in }
        .padding(Spacing.l)
    )
  }

  /// The middle of the picture, clear of everything on its corners.
  private static let middle = (
    x: margin + Int(PanelLayout.picture.width) / 2, y: margin + Int(PanelLayout.picture.height) / 2
  )

  /// A point on the badge's disc, beside its "!".
  private static let badge = (
    x: margin + Int(PanelLayout.captionInset) + 4,
    y: margin + Int(PanelLayout.captionInset + PanelLayout.badge / 2)
  )

  @Test("the panel's picture shows the control rim only while you have control")
  func theRimShowsOnlyInControl() throws {
    // Outside the picture's left edge, half way down: the rim's and the ring's band, wherever the rim settles.
    let band = (Self.margin - 6)..<Self.margin
    for look in [PanelPictureState.Look.live, .control, .needsYou, .paused] {
      let drawn = try Self.draw(look)
      let isBlue = band.contains { x in
        let pixel = drawn.colour(x: x, y: Self.middle.y)
        return pixel.alpha > 0.5 && pixel.blue > 0.8 && pixel.red < 0.3
      }
      #expect(isBlue == (look == .control), "\(look)")
    }
  }

  @Test("needs you desaturates the picture and adds the badge, and paused dims it without one")
  func needsYouAndPausedDim() throws {
    let live = try Self.draw(.live)
    let needsYou = try Self.draw(.needsYou)
    let paused = try Self.draw(.paused)
    let colour = live.colour(x: Self.middle.x, y: Self.middle.y)
    for (name, drawn) in [("needs you", needsYou), ("paused", paused)] {
      let dimmed = drawn.colour(x: Self.middle.x, y: Self.middle.y)
      #expect(PanelPixels.chroma(dimmed) < PanelPixels.chroma(colour) * 0.6, "\(name) greys the picture")
      #expect(max(dimmed.red, dimmed.green, dimmed.blue) < colour.red * 0.85, "\(name) darkens the picture")
    }
    let isOrange = { (pixel: (red: Double, green: Double, blue: Double, alpha: Double)) in
      pixel.red > 0.9 && pixel.green > 0.5 && pixel.green < 0.75 && pixel.blue < 0.2
    }
    #expect(isOrange(needsYou.colour(x: Self.badge.x, y: Self.badge.y)), "needs you carries the orange badge")
    #expect(!isOrange(paused.colour(x: Self.badge.x, y: Self.badge.y)), "paused carries none")
    #expect(!isOrange(live.colour(x: Self.badge.x, y: Self.badge.y)), "the live picture carries none")
  }

  @Test("the control row draws Stop before the main action only when its state shows it")
  func theRowDrawsStopOnlyWhenShown() throws {
    let main = PanelMainButton(title: "Open Atlas", help: "", event: .openAtlas)
    // A point inside the row's leading capsule: Stop's glass, or the white main action that fills the row.
    let lead = (x: Int(ControlHeight.sPlus) / 2, y: Int(ControlHeight.sPlus) / 2)
    for showsStop in [false, true] {
      let pixel = try PanelPixels(
        PanelControlRow(controls: PanelControls(showsStop: showsStop, main: main)) { _ in }
          .frame(width: PanelLayout.picture.width)
      )
      .colour(x: lead.x, y: lead.y)
      let isWhite = pixel.alpha > 0.9 && min(pixel.red, pixel.green, pixel.blue) > 0.9
      #expect(isWhite == !showsStop, "Stop \(showsStop ? "leads" : "is gone")")
    }
  }
}
