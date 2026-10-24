import AppKit
import AtlasStatus
import Testing

@testable import AtlasApp

@Suite("StatusMotion")
struct StatusMotionTests {
  /// How close a spring's duration or bounce must come to Motion.pop's.
  private static let tolerance = 1e-9
  /// The keys of the animations a state change adds to the badge.
  private static let changes = StatusBadge.changeKeys

  /// An item in a bar of its own, which never shows in the person's menu bar.
  ///
  /// - Parameters:
  ///   - reduceMotion: Whether Reduce Motion reads as on.
  ///   - tracking: Whether the run loop reads as tracking events, as while a menu is open.
  /// - Returns: The item, showing Idle.
  private static func item(reduceMotion: Bool, tracking: @escaping () -> Bool = { false }) -> StatusItem {
    let item = StatusItem(bar: NSStatusBar(), reduceMotion: { reduceMotion }, tracking: tracking)
    item.show(.idle, description: "Atlas")
    return item
  }

  /// The animations a state change has added to the badge, by key.
  ///
  /// - Parameter item: The item.
  /// - Returns: The keys of the pop and the crossfades that run.
  private static func changes(_ item: StatusItem) -> [String] {
    changes.filter { item.badge.layer.animation(forKey: $0) != nil }
  }

  /// The orbit's dim fade on an item's button, if one runs.
  ///
  /// - Parameter item: The item.
  /// - Returns: The fade.
  private static func dim(_ item: StatusItem) -> CABasicAnimation? {
    item.button?.layer?.animation(forKey: StatusItem.dimKey) as? CABasicAnimation
  }

  /// Whether an animation's value is a colour.
  ///
  /// - Parameters:
  ///   - value: The value.
  ///   - colour: The colour.
  /// - Returns: True when both are there and equal.
  private static func same(_ value: Any?, _ colour: CGColor?) -> Bool {
    guard let value, let colour else { return false }
    return CFEqual(value as CFTypeRef, colour)
  }

  @Test("the badge's spring matches Motion.pop")
  func theSpringIsMotionPop() throws {
    let item = Self.item(reduceMotion: false)
    item.show(.working, description: "Atlas")
    let pop = try #require(item.badge.layer.animation(forKey: StatusBadge.popKey) as? CASpringAnimation)
    #expect(pop.keyPath == "transform.scale")
    #expect(pop.fromValue as? CGFloat == 0)
    #expect(pop.toValue as? CGFloat == 1)
    // Undamped, the mass swings once in 2π √(m / k), the perceptual duration; the damping ratio c / (2 √(k m)) is
    // 1 − bounce.
    let duration = 2 * .pi * sqrt(pop.mass / pop.stiffness)
    let bounce = 1 - pop.damping / (2 * (pop.stiffness * pop.mass).squareRoot())
    #expect(abs(duration - 0.35) < Self.tolerance)
    #expect(abs(bounce - 0.15) < Self.tolerance)
    // Core Animation's own conversion agrees.
    let apple = CASpringAnimation(perceptualDuration: 0.35, bounce: 0.15)
    #expect(abs(pop.mass - apple.mass) < 1e-6)
    #expect(abs(pop.stiffness - apple.stiffness) < 1e-6)
    #expect(abs(pop.damping - apple.damping) < 1e-6)
    #expect(pop.duration == pop.settlingDuration, "the overshoot plays out")
  }

  @Test("Reduce Motion swaps the badge without a pop")
  func reduceMotionSwapsAtOnce() {
    let item = Self.item(reduceMotion: true)
    for kind in [Mode.Kind.working, .needsYou, .stopped, .connecting, .idle] {
      item.show(kind, description: "Atlas")
      #expect(Self.changes(item).isEmpty, "\(kind)")
      #expect(Self.dim(item) == nil, "\(kind)")
      #expect(item.badge.layer.animationKeys() == nil, "\(kind): no pop, crossfade or breath")
    }
    item.show(.stopped, description: "Atlas")
    #expect(!item.badge.layer.isHidden, "the badge shows whole at once")
    item.show(.loggedOut, description: "Atlas")
    #expect(item.button?.alphaValue == StatusGlyph.dimAlpha, "the dim lands at once")
  }

  @Test("a badge pops in, and the breath and the pop move different properties")
  func theBadgePopsIn() {
    let item = Self.item(reduceMotion: false)
    #expect(Self.changes(item).isEmpty, "Idle has no badge to pop")
    item.show(.working, description: "Atlas")
    #expect(Self.changes(item) == [StatusBadge.popKey])
    let breath = item.badge.layer.animation(forKey: StatusBadge.breathKey) as? CABasicAnimation
    #expect(breath?.keyPath == "opacity", "the breath fades while the pop scales")
    item.show(.idle, description: "Atlas")
    #expect(item.badge.layer.isHidden)
    #expect(item.badge.layer.animationKeys() == nil, "a badge leaving goes at once")
  }

  @Test("a new badge crossfades its colour, and its shape between disc and square, over 0.2 seconds")
  func theBadgeCrossfades() throws {
    let item = Self.item(reduceMotion: false)
    item.show(.working, description: "Atlas")
    item.show(.needsYou, description: "Atlas")
    #expect(
      Self.changes(item) == [StatusBadge.popKey, StatusBadge.colourKey, StatusBadge.settleKey],
      "blue to orange, both discs: the first pop goes on, no shape change, and the breath settles"
    )
    let colour = try #require(item.badge.layer.animation(forKey: StatusBadge.colourKey) as? CABasicAnimation)
    #expect(colour.keyPath == "fillColor")
    #expect(colour.duration == 0.2)
    #expect(Self.same(colour.toValue, item.badge.layer.fillColor))
    #expect(!Self.same(colour.fromValue, item.badge.layer.fillColor))
    item.show(.stopped, description: "Atlas")
    #expect(
      Self.changes(item) == [StatusBadge.popKey, StatusBadge.shapeKey],
      "orange to orange: the disc turns square"
    )
    let shape = try #require(item.badge.layer.animation(forKey: StatusBadge.shapeKey) as? CABasicAnimation)
    #expect(shape.keyPath == "path")
    #expect(shape.duration == 0.2)
  }

  @Test("the disc and the square share one shape of path, so one turns into the other")
  func theOutlinesMatch() throws {
    let box = try #require(StatusGlyph.frame(of: .accentDot))
    let local = CGRect(origin: .zero, size: box.size)
    let disc = StatusGlyph.outline(of: .attentionDot, in: local)
    let square = StatusGlyph.outline(of: .attentionSquare, in: local)
    var discElements: [CGPathElementType] = []
    var squareElements: [CGPathElementType] = []
    disc.applyWithBlock { discElements.append($0.pointee.type) }
    square.applyWithBlock { squareElements.append($0.pointee.type) }
    #expect(discElements == squareElements)
    #expect(disc != square)
    #expect(disc.boundingBoxOfPath == local, "the disc fills its box")
    #expect(disc.contains(CGPoint(x: local.midX, y: local.midY)))
    #expect(!disc.contains(CGPoint(x: 0.3, y: 0.3)), "the disc is round: its corners are empty")
    #expect(square.contains(CGPoint(x: 0.8, y: 0.8)), "the square's corners are only a little rounded")
  }

  @Test("the orbit's dim fades in and out over a quarter second, on the button's layer, after the first state")
  func theDimFades() throws {
    let first = StatusItem(bar: NSStatusBar()) { false }
    first.show(.connecting, description: "Atlas")
    #expect(Self.dim(first) == nil, "the first state shows at once")
    #expect(first.button?.alphaValue == StatusGlyph.dimAlpha)
    let item = Self.item(reduceMotion: false)
    item.show(.connecting, description: "Atlas")
    let fadeOut = try #require(Self.dim(item))
    #expect(fadeOut.keyPath == "opacity")
    #expect(fadeOut.fromValue as? Float == 1)
    #expect(fadeOut.toValue as? Float == Float(StatusGlyph.dimAlpha))
    #expect(fadeOut.duration == 0.25)
    #expect(item.button?.alphaValue == StatusGlyph.dimAlpha)
    #expect(item.button?.layer?.animationKeys() == [StatusItem.dimKey], "AppKit adds no fade of its own")
    item.show(.idle, description: "Atlas")
    let fadeIn = try #require(Self.dim(item))
    #expect(fadeIn.fromValue as? Float == Float(StatusGlyph.dimAlpha))
    #expect(fadeIn.toValue as? Float == 1)
    #expect(item.button?.alphaValue == 1)
    item.button?.layer?.removeAllAnimations()
    item.show(.working, description: "Atlas")
    #expect(Self.dim(item) == nil, "Idle and Working are both whole")
  }

  @Test("a menu opening adds no animation, and a state change while it is open swaps in at once")
  func aMenuOpeningMovesNothing() {
    var tracking = false
    let item = Self.item(reduceMotion: false) { tracking }
    item.show(.connecting, description: "Atlas")
    item.show(.working, description: "Atlas")
    #expect(Self.dim(item) != nil)
    #expect(item.badge.layer.animation(forKey: StatusBadge.popKey) != nil)
    let menu = NSMenu()
    let centre = NotificationCenter.default
    tracking = true
    centre.post(name: NSMenu.didBeginTrackingNotification, object: menu)
    #expect(item.badge.layer.animationKeys() == [StatusBadge.breathKey], "the pop stops; the breath runs on")
    #expect(item.button?.layer?.animationKeys() == nil, "the dim's fade stops, its model value final")
    #expect(item.button?.alphaValue == 1)
    item.show(.idle, description: "Atlas")
    item.show(.needsYou, description: "Atlas")
    #expect(Self.changes(item).isEmpty, "no pop while the menu is open")
    #expect(!item.badge.layer.isHidden)
    item.show(.connecting, description: "Atlas")
    item.show(.idle, description: "Atlas")
    #expect(Self.dim(item) == nil, "no fade while the menu is open")
    #expect(item.button?.alphaValue == 1)
    tracking = false
    centre.post(name: NSMenu.didEndTrackingNotification, object: menu)
    item.show(.needsYou, description: "Atlas")
    #expect(Self.changes(item) == [StatusBadge.popKey], "once the menu closes, a badge pops again")
  }

  @Test("while a menu is open a running breath keeps running and none starts, until the menu closes")
  func theBreathWaitsForTheMenu() {
    var tracking = false
    let item = Self.item(reduceMotion: false) { tracking }
    let menu = NSMenu()
    let centre = NotificationCenter.default
    item.show(.working, description: "Atlas")
    tracking = true
    centre.post(name: NSMenu.didBeginTrackingNotification, object: menu)
    #expect(item.badge.layer.animation(forKey: StatusBadge.breathKey) != nil, "a running breath keeps running")
    item.show(.idle, description: "Atlas")
    item.show(.working, description: "Atlas")
    #expect(item.badge.layer.animation(forKey: StatusBadge.breathKey) == nil, "no breath starts while it is open")
    tracking = false
    centre.post(name: NSMenu.didEndTrackingNotification, object: menu)
    #expect(item.badge.layer.animation(forKey: StatusBadge.breathKey) != nil, "the breath starts as it closes")
  }

  @Test("a menu whose end never came stops holding the item still once the run loop stops tracking")
  func aLostMenuEndRecovers() {
    var tracking = true
    let item = Self.item(reduceMotion: false) { tracking }
    let centre = NotificationCenter.default
    centre.post(name: NSMenu.didBeginTrackingNotification, object: NSMenu())
    #expect(item.motion == .breath)
    tracking = false
    #expect(item.motion == .all)
    tracking = true
    #expect(item.motion == .all, "the count started again from none")
  }

  @Test("a change during a pop lets the pop finish, and a badge leaving mid-pop goes whole and at once")
  func aChangeMidPopKeepsThePop() {
    let item = Self.item(reduceMotion: false)
    item.show(.working, description: "Atlas")
    item.show(.stopped, description: "Atlas")
    #expect(item.badge.layer.animation(forKey: StatusBadge.popKey) != nil, "the pop keeps running")
    #expect(item.badge.layer.animation(forKey: StatusBadge.shapeKey) != nil)
    item.show(.idle, description: "Atlas")
    #expect(item.badge.layer.isHidden)
    #expect(item.badge.layer.animationKeys() == nil)
    #expect(item.badge.layer.value(forKeyPath: "transform.scale") as? CGFloat == 1)
  }

  @Test("when the breath stops its opacity settles back to whole over 0.2 seconds, and at once under Reduce Motion")
  func theBreathSettles() throws {
    let item = Self.item(reduceMotion: false)
    item.show(.working, description: "Atlas")
    item.show(.stopped, description: "Atlas")
    #expect(item.badge.layer.animation(forKey: StatusBadge.breathKey) == nil)
    let settle = try #require(item.badge.layer.animation(forKey: StatusBadge.settleKey) as? CABasicAnimation)
    #expect(settle.keyPath == "opacity")
    #expect(settle.toValue as? Float == 1)
    #expect(settle.duration == 0.2)
    #expect(item.badge.layer.opacity == 1)
    var reduceMotion = false
    let still = StatusItem(bar: NSStatusBar(), reduceMotion: { reduceMotion }, tracking: { false })
    still.show(.working, description: "Atlas")
    reduceMotion = true
    still.show(.stopped, description: "Atlas")
    #expect(still.badge.layer.animationKeys() == nil)
  }

  @Test("laying the same badge out again adds no animation")
  func layingOutAgainMovesNothing() {
    let item = Self.item(reduceMotion: false)
    item.show(.stopped, description: "Atlas")
    item.badge.layer.removeAllAnimations()
    item.button?.layer?.removeAllAnimations()
    item.showBadge()
    item.show(.stopped, description: "Atlas")
    #expect(item.badge.layer.animationKeys() == nil)
    #expect(item.button?.layer?.animationKeys() == nil)
  }

  @Test("what may move: nothing under Reduce Motion, only the breath while a menu is open, otherwise everything")
  func motionFollowsReduceMotionAndMenus() {
    #expect(StatusMotion.now(reduceMotion: false, menuOpen: false) == .all)
    #expect(StatusMotion.now(reduceMotion: false, menuOpen: true) == .breath)
    #expect(StatusMotion.now(reduceMotion: true, menuOpen: false) == .still)
    #expect(StatusMotion.now(reduceMotion: true, menuOpen: true) == .still)
  }
}
