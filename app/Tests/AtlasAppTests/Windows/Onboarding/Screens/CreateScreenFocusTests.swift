import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

/// Here, not in AtlasUI's tests, because the focus lives in AppKit's responder chain, which AtlasUI never imports.
@MainActor
@Suite("CreateScreen focus")
struct CreateScreenFocusTests {
  @Test("a flagged account name washes its row and takes the focus")
  func flaggedNameTakesTheFocus() async throws {
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: OnboardingLayout.card),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    let form = OnboardingAccountForm(fullName: "Helper", accountName: "Atlas Helper", password: "a", verify: "a")
    let flagged = OnboardingCreateScreenState(
      tail: "You'll approve it once in macOS.",
      note: .problem("Account names can't have spaces."),
      isFormEnabled: true,
      flagged: .accountName
    )
    // Drawn settled, as a render draws it, so the form shows whole at once.
    let host = NSHostingView(
      rootView: CreateScreen(state: flagged, form: .constant(form))
        .environment(\.animatesArrivals, false)
    )
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    // The field takes the focus a turn after it appears; a few turns at most, on the main actor, never a clock.
    for _ in 0..<Self.turns where !(window.firstResponder is NSText) {
      await Task.yield()
    }
    let editor = try #require(window.firstResponder as? NSText, "a field has the focus")
    let field = try #require(editor.delegate as? NSTextField)
    #expect(field.stringValue == "Atlas Helper", "the account name has the focus, not \(field.stringValue)")

    // The flagged row washes orange where nothing is drawn but the row: in the gap between its name and its field.
    let frame = field.convert(field.bounds, to: host)
    let point = NSPoint(x: frame.minX - 6, y: frame.midY)
    let above = NSPoint(x: point.x, y: point.y + (host.isFlipped ? -40 : 40))
    let picture = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
    host.cacheDisplay(in: host.bounds, to: picture)
    let wash = try #require(colour(picture, at: point, in: host))
    let plain = try #require(colour(picture, at: above, in: host))
    #expect(wash.redComponent > wash.blueComponent + 0.03, "the flagged row is washed orange: \(wash)")
    #expect(abs(plain.redComponent - plain.blueComponent) < 0.02, "the row above is not: \(plain)")
  }

  /// How many turns of the main actor the focus may take to land.
  private static let turns = 20

  /// The colour a view drew at a point.
  ///
  /// - Parameters:
  ///   - picture: The view's drawing.
  ///   - point: The point, in the view's coordinates.
  ///   - view: The view.
  /// - Returns: The colour, in sRGB.
  private func colour(_ picture: NSBitmapImageRep, at point: NSPoint, in view: NSView) -> NSColor? {
    let scale = CGFloat(picture.pixelsWide) / view.bounds.width
    let top = view.isFlipped ? point.y : view.bounds.height - point.y
    return picture.colorAt(x: Int(point.x * scale), y: Int(top * scale))?
      .usingColorSpace(.sRGB)
  }
}
