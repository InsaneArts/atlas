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

  @Test("a failed newest action leads the picture's caption with an orange dot, any other with the steady blue one")
  func aFailedCaptionIsOrange() throws {
    let working = PanelCaption(text: "Clicked Save", time: "just now")
    let failed = PanelCaption(text: "Clicked Save", time: "just now", isFailed: true)
    #expect(PanelPicture.lead(working) == PictureChip<EmptyView>.Lead.dot(.active))
    #expect(PanelPicture.lead(failed) == PictureChip<EmptyView>.Lead.dot(.attention))
    // The chip's leading end: past the caption's corner inset, across the small chip's end inset and its dot, on the
    // chip's middle line.
    let left = Self.margin + Int(PanelLayout.captionInset)
    let across = left..<(left + Int(Spacing.sPlus) + Int(IconSize.xxs) * 2)
    let middle = Self.margin + Int(PanelLayout.picture.height - PanelLayout.captionInset - ControlHeight.xsPlus / 2)
    func marks(_ caption: PanelCaption) throws -> (isBlue: Bool, isOrange: Bool) {
      let state = PanelPictureState(
        look: .live,
        caption: caption,
        pause: PanelPause(isPaused: false, label: "Pause picture", help: "")
      )
      let drawn = try PanelPixels(
        PanelPicture(state: state) { _ in }
          .padding(Spacing.l)
      )
      let rows = (middle - 2)...(middle + 2)
      let pixels = across.flatMap { x in rows.map { y in drawn.colour(x: x, y: y) } }
      return (
        // The breathing dot may draw at a low point of its breath, so blue need only lead clearly.
        pixels.contains { $0.blue > 0.4 && $0.blue > $0.red * 2 && $0.blue > $0.green * 1.3 },
        pixels.contains { $0.red > 0.85 && $0.green > 0.45 && $0.green < 0.8 && $0.blue < 0.3 }
      )
    }
    let busy = try marks(working)
    let failure = try marks(failed)
    #expect(busy.isBlue && !busy.isOrange, "work under way breathes blue")
    #expect(failure.isOrange && !failure.isBlue, "a failure is orange")
  }

  @Test("the pause circle's bars and its play triangle both stand centred in the circle")
  func thePauseSymbolsStandCentred() throws {
    let side = Int(PanelLayout.pauseButton)
    let left = Self.margin + Int(PanelLayout.picture.width - PanelLayout.captionInset) - side
    let top = Self.margin + Int(PanelLayout.picture.height - PanelLayout.captionInset) - side
    for look in [PanelPictureState.Look.live, .paused] {
      let drawn = try Self.draw(look)
      // The symbol is drawn light on the circle's dark fill; the red picture round the circle is not light.
      let inked = (top..<(top + side))
        .flatMap { y in
          (left..<(left + side))
            .filter { x in
              let pixel = drawn.colour(x: x, y: y)
              return min(pixel.red, pixel.green, pixel.blue) > 0.6
            }
        }
      try #require(!inked.isEmpty, "\(look): no symbol drawn")
      // By the ink's weight rather than its box, so the triangle's point does not pull its middle.
      let centre = Double(inked.reduce(0, +)) / Double(inked.count) + 0.5
      #expect(abs(centre - (Double(left) + Double(side) / 2)) <= 1, "\(look): \(centre)")
    }
  }

  @Test("the control row draws Stop before the main action only when its state shows it")
  func theRowDrawsStopOnlyWhenShown() throws {
    let main = PanelMainButton(title: "Open Atlas", help: "", event: .openAtlas)
    // A point inside the row's leading capsule: Stop's quiet fill, or the white main action that fills the row.
    let lead = (x: Int(ControlHeight.sPlus) / 2, y: Int(ControlHeight.sPlus) / 2)
    for showsStop in [false, true] {
      let pixel = try PanelPixels(
        PanelControlRow(controls: PanelControls(agent: showsStop ? .stop : nil, main: main)) { _ in }
          .frame(width: PanelLayout.picture.width)
      )
      .colour(x: lead.x, y: lead.y)
      let isWhite = pixel.alpha > 0.9 && min(pixel.red, pixel.green, pixel.blue) > 0.9
      #expect(isWhite == !showsStop, "Stop \(showsStop ? "leads" : "is gone")")
    }
  }

  @Test("pressing Stop turns it into Resume without changing its width, so the main action never moves")
  func stopAndResumeShareOneWidth() throws {
    let main = PanelMainButton(title: "Open Atlas", help: "", event: .openAtlas)
    let middle = Int(ControlHeight.sPlus) / 2
    func mainStart(_ agent: ViewerAgentButton) throws -> Int? {
      let pixels = try PanelPixels(
        PanelControlRow(controls: PanelControls(agent: agent, main: main)) { _ in }
          .frame(width: PanelLayout.picture.width)
      )
      func isWhite(_ x: Int) -> Bool {
        let pixel = pixels.colour(x: x, y: middle)
        return pixel.alpha > 0.9 && min(pixel.red, pixel.green, pixel.blue) > 0.9
      }
      // The white fill, a long run of white, not the secondary button's white words.
      let run = 24
      let starts = 0..<(pixels.width - run)
      return starts.first { x in
        let span = x..<(x + run)
        return span.allSatisfy(isWhite)
      }
    }
    let stop = try mainStart(.stop)
    let resume = try mainStart(.resume(isEnabled: true))
    #expect(stop != nil, "the white main action is drawn")
    #expect(resume == stop)
  }
}
