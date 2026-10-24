import AppKit
import AtlasStatus
import Testing

@testable import AtlasApp

@Suite("StatusGlyph")
struct StatusGlyphTests {
  /// The look of every state.
  private static let looks = Mode.Kind.allCases
    .map { (kind: $0, look: StatusGlyph.look(for: $0)) }

  /// An image's alpha, two pixels a point, by pixel.
  ///
  /// - Parameter image: The image.
  /// - Returns: Its bitmap.
  /// - Throws: When no bitmap could be made.
  private static func bitmap(_ image: NSImage) throws -> NSBitmapImageRep {
    let side = Int(image.size.width) * 2
    let bitmap = try #require(
      NSBitmapImageRep(
        bitmapDataPlanes: nil,
        pixelsWide: side,
        pixelsHigh: side,
        bitsPerSample: 8,
        samplesPerPixel: 4,
        hasAlpha: true,
        isPlanar: false,
        colorSpaceName: .deviceRGB,
        bytesPerRow: 0,
        bitsPerPixel: 0
      )
    )
    bitmap.size = image.size
    let context = try #require(NSGraphicsContext(bitmapImageRep: bitmap))
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = context
    image.draw(in: NSRect(origin: .zero, size: image.size))
    NSGraphicsContext.restoreGraphicsState()
    return bitmap
  }

  @Test("the status item is 22 points wide in every state")
  func theItemKeepsOneWidth() throws {
    // A bar of its own, which never shows in the person's menu bar.
    let item = StatusItem(bar: NSStatusBar()) { false }
    #expect(item.length == 22)
    for (kind, look) in Self.looks {
      item.show(kind, description: "Atlas")
      #expect(item.length == 22, "\(kind)")
      let image = StatusGlyph.image(for: look)
      #expect(image.size == NSSize(width: 22, height: 22), "\(kind)")
      if let badge = StatusGlyph.frame(of: look.badge) {
        #expect(badge.minX >= 0 && badge.maxX <= 22, "\(kind)'s badge stays inside the item")
      }
    }
  }

  @Test("control tints the whole orbit blue and carries no badge")
  func controlIsABlueOrbit() {
    let look = StatusGlyph.look(for: .control)
    #expect(look == StatusGlyph.Look(tint: .accent, badge: .none, breathes: false))
    #expect(!StatusGlyph.image(for: look).isTemplate)
    for (kind, look) in Self.looks where kind != .control {
      #expect(look.tint != .accent, "\(kind)")
      #expect(StatusGlyph.image(for: look).isTemplate, "\(kind) keeps the template look")
    }
  }

  @Test("logged out, not ready and connecting dim the orbit and carry no badge")
  func theQuietStatesDim() {
    let quiet: [Mode.Kind] = [.loggedOut, .notReady, .connecting]
    for (kind, look) in Self.looks {
      #expect((look.tint == .dimmed) == quiet.contains(kind), "\(kind)")
      if quiet.contains(kind) { #expect(look.badge == .none, "\(kind)") }
    }
    #expect(StatusGlyph.dimAlpha == 0.42)
    #expect(StatusGlyph.opacity(of: .dimmed) == 0.42)
    #expect(StatusGlyph.opacity(of: .template) == 1 && StatusGlyph.opacity(of: .accent) == 1)
  }

  @Test("only working, needs you and stopped carry a badge, and stopped's is a square")
  func onlyThreeStatesCarryABadge() throws {
    let badges = Dictionary(uniqueKeysWithValues: Self.looks.map { ($0.kind, $0.look.badge) })
    #expect(badges[.working] == .accentDot)
    #expect(badges[.needsYou] == .attentionDot)
    #expect(badges[.stopped] == .attentionSquare)
    for (kind, badge) in badges where ![.working, .needsYou, .stopped].contains(kind) {
      #expect(badge == .none, "\(kind)")
      #expect(StatusGlyph.frame(of: badge) == nil, "\(kind)")
    }
    #expect(try #require(StatusGlyph.frame(of: .accentDot)).width == 7)
    let square = try #require(StatusGlyph.frame(of: .attentionSquare))
    #expect(square == StatusGlyph.frame(of: .accentDot), "the square shares the disc's box")
    #expect(square.width == 7)
    #expect(square == square.integral, "on whole points, so whole pixels at one pixel a point")
    #expect(StatusGlyph.colour(of: .accentDot) == .systemBlue)
    #expect(StatusGlyph.colour(of: .attentionSquare) == .systemOrange)
  }

  @Test("the badge's ring is knocked out of the orbit, so the menu bar shows through around it")
  func theRingIsAKnockout() throws {
    let plain = try Self.bitmap(StatusGlyph.image(for: StatusGlyph.look(for: .idle)))
    let cut = try Self.bitmap(StatusGlyph.image(for: StatusGlyph.look(for: .needsYou)))
    let badge = try #require(StatusGlyph.frame(of: .attentionDot))
    // The ring's outer circle, top down, 1.5 past the badge; the bitmap runs top down too.
    let ring = badge.insetBy(dx: -1.5, dy: -1.5)
    var inkedBefore = 0
    var inkedAfter = 0
    for row in 0..<plain.pixelsHigh {
      for column in 0..<plain.pixelsWide {
        let point = CGPoint(x: (CGFloat(column) + 0.5) / 2, y: (CGFloat(row) + 0.5) / 2)
        let offset = CGPoint(x: point.x - ring.midX, y: point.y - ring.midY)
        // Half a pixel in from the edge, where antialiasing is.
        guard (offset.x * offset.x + offset.y * offset.y).squareRoot() < ring.width / 2 - 0.5 else { continue }
        if (plain.colorAt(x: column, y: row)?.alphaComponent ?? 0) > 0.5 { inkedBefore += 1 }
        if (cut.colorAt(x: column, y: row)?.alphaComponent ?? 0) > 0.05 { inkedAfter += 1 }
      }
    }
    #expect(inkedBefore > 0, "the orbit reaches under the badge")
    #expect(inkedAfter == 0)
  }

  @Test("the badge breathes only while an action runs")
  func onlyWorkingBreathes() {
    for (kind, look) in Self.looks {
      #expect(look.breathes == (kind == .working), "\(kind)")
    }
    let badge = StatusBadge()
    let bounds = CGRect(x: 0, y: 0, width: 22, height: 24)
    let dark = NSAppearance(named: .darkAqua) ?? NSAppearance.currentDrawing()
    badge.show(StatusGlyph.look(for: .working), in: bounds, flipped: true, appearance: dark, motion: .all)
    let breath = badge.layer.animation(forKey: StatusBadge.breathKey) as? CABasicAnimation
    #expect(breath?.keyPath == "opacity")
    #expect(breath?.fromValue as? Int == 1)
    #expect(breath?.toValue as? Double == 0.5)
    #expect(breath?.autoreverses == true)
    #expect(breath?.repeatCount == .infinity)
    #expect((breath?.duration ?? 0) * 2 == 1.6, "one breath in and out takes 1.6 seconds")
    badge.show(StatusGlyph.look(for: .needsYou), in: bounds, flipped: true, appearance: dark, motion: .all)
    #expect(badge.layer.animation(forKey: StatusBadge.breathKey) == nil)
    #expect(!badge.layer.isHidden)
    badge.show(StatusGlyph.look(for: .idle), in: bounds, flipped: true, appearance: dark, motion: .all)
    #expect(badge.layer.isHidden)
  }

  @Test("Reduce Motion keeps the badge steady")
  func reduceMotionHoldsTheBadgeStill() {
    let badge = StatusBadge()
    let bounds = CGRect(x: 0, y: 0, width: 22, height: 22)
    let dark = NSAppearance(named: .darkAqua) ?? NSAppearance.currentDrawing()
    let working = StatusGlyph.look(for: .working)
    badge.show(working, in: bounds, flipped: true, appearance: dark, motion: .still)
    #expect(badge.layer.animation(forKey: StatusBadge.breathKey) == nil)
    #expect(!badge.layer.isHidden)
    badge.show(working, in: bounds, flipped: true, appearance: dark, motion: .all)
    #expect(badge.layer.animation(forKey: StatusBadge.breathKey) != nil)
    badge.show(working, in: bounds, flipped: true, appearance: dark, motion: .still)
    #expect(badge.layer.animation(forKey: StatusBadge.breathKey) == nil, "turning Reduce Motion on stops a breath")
  }

  @Test("the badge sits at the orbit's lower right whichever way the button's layer runs")
  func theBadgeSitsInTheSamePlaceFlippedOrNot() throws {
    let box = try #require(StatusGlyph.frame(of: .attentionDot))
    // The image as a 24-point-high button draws it: one point down from its top.
    let image = CGRect(x: 0, y: 1, width: 22, height: 22)
    let flipped = StatusBadge.place(box, in: image, flipped: true)
    let upright = StatusBadge.place(box, in: image, flipped: false)
    #expect(flipped.minX == box.minX && upright.minX == box.minX)
    #expect(flipped.minY == 1 + box.minY)
    #expect(upright.minY == 24 - flipped.maxY, "the same place counted from the bottom")
  }

  @Test("turning Reduce Motion on stops the breath the next time the item lays its badge out, and off starts it")
  func theItemReadsReduceMotionEachTime() {
    var reduceMotion = false
    let item = StatusItem(bar: NSStatusBar()) { reduceMotion }
    item.show(.working, description: "Atlas")
    #expect(item.badge.layer.animation(forKey: StatusBadge.breathKey) != nil)
    reduceMotion = true
    item.showBadge()
    #expect(item.badge.layer.animation(forKey: StatusBadge.breathKey) == nil)
    #expect(!item.badge.layer.isHidden, "the badge stays, steady")
    reduceMotion = false
    item.showBadge()
    #expect(item.badge.layer.animation(forKey: StatusBadge.breathKey) != nil)
  }
}
