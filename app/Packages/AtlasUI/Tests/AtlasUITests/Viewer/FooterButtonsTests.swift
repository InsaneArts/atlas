import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Footer buttons")
@MainActor
struct FooterButtonsTests {
  @Test("the main footer button keeps one width through Take Control, Connecting, Give Back and Recover")
  func theMainButtonKeepsOneWidth() throws {
    let labels = PreviewStates.viewerMainLabels
    for fit in [ViewerFooterFit.full, .compact] {
      let buttons = [
        PreviewStates.viewerTakeControl(isEnabled: true),
        Self.connecting,
        PreviewStates.viewerGiveBack,
        PreviewStates.viewerRecover,
      ]
      let widths = try buttons.map { button in
        try PanelPixels(ViewerMainButton(control: button, fold: FooterFold(fit)) { _ in }).width
      }
      #expect(Set(widths).count == 1, "\(fit): \(widths)")
      #expect(buttons.allSatisfy { $0.room == labels }, "every label shares the one room")
    }
  }

  @Test("Stop and Resume stand at one width, so the pause beside them never moves")
  func stopAndResumeKeepOneWidth() throws {
    for fit in [ViewerFooterFit.full, .compact] {
      let stop = try PanelPixels(FooterAgentButton(agent: .stop, help: "", fold: FooterFold(fit)) { _ in })
      let resume = try PanelPixels(
        FooterAgentButton(agent: .resume(isEnabled: true), help: "", fold: FooterFold(fit)) { _ in }
      )
      #expect(stop.width == resume.width, "\(fit)")
    }
  }

  @Test("the pause circle's bars and its play triangle both stand centred in the circle")
  func thePauseSymbolsStandCentred() throws {
    for paused in [false, true] {
      let pause = ViewerToggle(isOn: paused, label: "", help: "")
      let drawn = try PanelPixels(ViewerPauseSlot(pause: pause) { _ in })
      let centre = try #require(Self.inkCentre(drawn), "\(paused): no symbol drawn")
      #expect(abs(centre - Double(drawn.width) / 2) <= Self.centredWithin, "\(paused): \(centre) in \(drawn.width)")
    }
  }

  /// Take Control while control is being asked for.
  private static var connecting: ViewerControlButton {
    var button = PreviewStates.viewerTakeControl(isEnabled: false)
    button.title = "Connecting…"
    return button
  }

  /// 1: how far a symbol's middle may stand from the circle's, in points, for pixel rounding.
  private static let centredWithin = 1.0

  /// Where a symbol's ink stands across, by its weight rather than its box, so a triangle's point does not pull its
  /// middle: the mean column of the points, over the drawing's middle band of rows, light enough to be the symbol.
  ///
  /// - Parameter drawn: The drawing.
  /// - Returns: The ink's middle, in points from the left edge of the drawing, or nil for none.
  private static func inkCentre(_ drawn: PanelPixels) -> Double? {
    let rows = (drawn.height / 3)..<(drawn.height * 2 / 3)
    let inked = rows.flatMap { y in
      (0..<drawn.width)
        .filter { x in
          let pixel = drawn.colour(x: x, y: y)
          return pixel.alpha > Self.inkAlpha && pixel.red > Self.inkLight
        }
    }
    guard !inked.isEmpty else { return nil }
    // A pixel's middle is half a point past its left edge.
    return Double(inked.reduce(0, +)) / Double(inked.count) + 0.5
  }

  /// 0.5: how opaque a point must be to count as the symbol's ink rather than its soft edge or the clear ground.
  private static let inkAlpha = 0.5
  /// 0.6: how light a point must be to count as the symbol, drawn light in the dark appearance the drawing uses.
  private static let inkLight = 0.6
}
