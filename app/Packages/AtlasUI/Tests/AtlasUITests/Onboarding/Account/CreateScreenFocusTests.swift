import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

/// The focus lives in AppKit's responder chain, which only this test target imports: AtlasUI itself never does.
/// One at a time: each test counts turns of the main actor, which another test's window would share.
@MainActor
@Suite("CreateScreen focus", .serialized)
struct CreateScreenFocusTests {
  @Test("a flagged account name washes its row and takes the focus")
  func flaggedNameTakesTheFocus() async throws {
    let (window, host) = Self.host(Self.flagged)
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

  @Test("a flag cleared before its field takes the focus moves the focus nowhere")
  func aClearedFlagTakesNoFocus() async throws {
    let (window, host) = Self.host(Self.flagged)
    // Cleared in the same turn the form appeared, before the flag's deferred focus has had its turn.
    var cleared = Self.flagged
    cleared.flagged = nil
    cleared.note = .none
    host.rootView = Self.screen(cleared)
    host.layoutSubtreeIfNeeded()
    for _ in 0..<Self.turns {
      await Task.yield()
    }
    #expect(!(window.firstResponder is NSText), "the cleared flag still took the focus")
  }

  @Test("a field the person focuses before the flag's field takes the focus keeps it")
  func aFieldFocusedMeanwhileKeepsTheFocus() async throws {
    let (window, host) = Self.host(Self.flagged)
    let password = try #require(Self.fields(in: host).first { $0 is NSSecureTextField }, "the password field")
    window.makeFirstResponder(password)
    host.layoutSubtreeIfNeeded()
    for _ in 0..<Self.turns {
      await Task.yield()
    }
    let editor = try #require(window.firstResponder as? NSText, "a field has the focus")
    #expect(editor.delegate === password, "the flag took the focus from the field the person typed in")
  }

  /// How many turns of the main actor the focus may take to land.
  private static let turns = 20

  /// The form with its account name flagged.
  private static let flagged = OnboardingCreateScreenState(
    tail: "You'll approve it once in macOS.",
    note: .problem("Account names can't have spaces."),
    isFormEnabled: true,
    flagged: .accountName
  )

  /// The form as `state` shows it, drawn settled, as a render draws it, so it shows whole at once.
  ///
  /// - Parameter state: What the screen shows around the form.
  /// - Returns: The screen.
  private static func screen(_ state: OnboardingCreateScreenState) -> AnyView {
    let form = OnboardingAccountForm(fullName: "Helper", accountName: "Atlas Helper", password: "a", verify: "a")
    return AnyView(
      CreateScreen(state: state, form: .constant(form))
        .environment(\.animatesArrivals, false)
    )
  }

  /// A window showing the form as `state` shows it, laid out.
  ///
  /// - Parameter state: What the screen shows around the form.
  /// - Returns: The window and the view hosting the form.
  private static func host(_ state: OnboardingCreateScreenState) -> (NSWindow, NSHostingView<AnyView>) {
    let window = NSWindow(
      contentRect: NSRect(origin: .zero, size: OnboardingLayout.card),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    let host = NSHostingView(rootView: screen(state))
    window.contentView = host
    host.layoutSubtreeIfNeeded()
    return (window, host)
  }

  /// Every text field under `view`, in the order AppKit holds them.
  ///
  /// - Parameter view: The view.
  /// - Returns: The fields.
  private static func fields(in view: NSView) -> [NSTextField] {
    view.subviews.flatMap { subview in
      [subview as? NSTextField].compactMap { $0 } + fields(in: subview)
    }
  }

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
