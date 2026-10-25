import AppKit
import AtlasStatus
import Testing

@testable import AtlasApp

@Suite("StatusGlyph", .serialized, AppKitState())
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

  @Test("the orbit is a template in every state, so it reads on a tinted menu bar, and control is a blue dot on it")
  func everyStateKeepsTheTemplate() {
    for (kind, look) in Self.looks {
      #expect(StatusGlyph.image(for: look).isTemplate, "\(kind)")
    }
    #expect(StatusGlyph.look(for: .control) == StatusGlyph.Look(tint: .template, badge: .accentDot, breathes: false))
  }

  @Test("only logged out and connecting dim the orbit")
  func theQuietStatesDim() {
    let quiet: [Mode.Kind] = [.loggedOut, .connecting]
    for (kind, look) in Self.looks {
      #expect((look.tint == .dimmed) == quiet.contains(kind), "\(kind)")
    }
    #expect(StatusGlyph.dimAlpha == 0.42)
    #expect(StatusGlyph.opacity(of: .dimmed) == 0.42)
    #expect(StatusGlyph.opacity(of: .template) == 1)
  }

  @Test("no two states look alike: each has its own tint and badge, and only idle and connecting carry no badge")
  func everyStateLooksItsOwn() throws {
    let badges = Dictionary(uniqueKeysWithValues: Self.looks.map { ($0.kind, $0.look.badge) })
    #expect(badges[.working] == .accentDot)
    #expect(badges[.control] == .accentDot)
    #expect(badges[.needsYou] == .attentionDot)
    #expect(badges[.stopped] == .attentionSquare)
    #expect(badges[.notReady] == .attentionRing)
    #expect(badges[.loggedOut] == .quietRing)
    #expect(badges[.inUseElsewhere] == .quietDot)
    #expect(badges[.idle] == StatusGlyph.Badge.none && badges[.connecting] == StatusGlyph.Badge.none)
    #expect(StatusGlyph.frame(of: .none) == nil)
    let looks = Self.looks
      .map(\.look)
    for (index, look) in looks.enumerated() {
      #expect(!looks[(index + 1)...].contains(look), "\(Self.looks[index].kind) shares its look")
    }
    #expect(StatusGlyph.colour(of: .quietRing) == .systemGray && StatusGlyph.colour(of: .quietDot) == .systemGray)
    #expect(StatusGlyph.colour(of: .attentionRing) == .systemOrange)
    #expect(StatusGlyph.isHollow(.attentionRing) && StatusGlyph.isHollow(.quietRing))
    #expect(!StatusGlyph.isHollow(.accentDot) && !StatusGlyph.isHollow(.attentionSquare))
    let ring = StatusGlyph.outline(of: .attentionRing, in: CGRect(x: 0, y: 0, width: 7, height: 7))
    #expect(ring.boundingBox == CGRect(x: 0.75, y: 0.75, width: 5.5, height: 5.5), "the stroke fills the box exactly")
    #expect(try #require(StatusGlyph.frame(of: .accentDot)).width == 7)
    let square = try #require(StatusGlyph.frame(of: .attentionSquare))
    #expect(square == StatusGlyph.frame(of: .accentDot), "the square shares the disc's box")
    #expect(square.width == 7)
    #expect(square == square.integral, "on whole points, so whole pixels at one pixel a point")
    #expect(StatusGlyph.colour(of: .accentDot) == .systemBlue)
    #expect(StatusGlyph.colour(of: .attentionSquare) == .systemOrange)
  }

  @Test("a 1-point gap is knocked out of the orbit round the badge, so the menu bar shows through around it")
  func theRingIsAKnockout() throws {
    let plain = try Self.bitmap(StatusGlyph.image(for: StatusGlyph.look(for: .idle)))
    let cut = try Self.bitmap(StatusGlyph.image(for: StatusGlyph.look(for: .needsYou)))
    let badge = try #require(StatusGlyph.frame(of: .attentionDot))
    // The gap's outer circle, top down, 1 past the badge; the bitmap runs top down too.
    let gap = badge.width / 2 + 1
    var inkedBefore = 0
    var inkedAfter = 0
    for row in 0..<plain.pixelsHigh {
      for column in 0..<plain.pixelsWide {
        let point = CGPoint(x: (CGFloat(column) + 0.5) / 2, y: (CGFloat(row) + 0.5) / 2)
        let distance = hypot(point.x - badge.midX, point.y - badge.midY)
        if distance < gap, (plain.colorAt(x: column, y: row)?.alphaComponent ?? 0) > 0.5 { inkedBefore += 1 }
        // A quarter point in from the edge, where antialiasing is.
        if distance < gap - 0.25, (cut.colorAt(x: column, y: row)?.alphaComponent ?? 0) > 0.05 { inkedAfter += 1 }
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
