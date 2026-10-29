import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

// On the main actor: a colour that follows the appearance resolves through AppKit, which answers on the main thread.
@Suite("Design system")
@MainActor
struct DesignSystemTests {
  /// The spacing scale, smallest first.
  private static let spacing: [CGFloat] = [
    Spacing.hairline, Spacing.half, Spacing.xs, Spacing.xsPlus, Spacing.s, Spacing.sPlus, Spacing.m, Spacing.mPlus,
    Spacing.l, Spacing.xl, Spacing.xxl, Spacing.xxxl,
  ]

  /// Every scale named by size, each step beside its name.
  private static let sizeScales: [(scale: String, steps: [(name: String, value: CGFloat)])] = [
    (
      "Spacing",
      [
        ("hairline", Spacing.hairline), ("half", Spacing.half), ("xs", Spacing.xs), ("xsPlus", Spacing.xsPlus),
        ("s", Spacing.s), ("sPlus", Spacing.sPlus), ("m", Spacing.m), ("mPlus", Spacing.mPlus), ("l", Spacing.l),
        ("xl", Spacing.xl), ("xxl", Spacing.xxl), ("xxxl", Spacing.xxxl),
      ]
    ),
    (
      "IconSize",
      [
        ("xxs", IconSize.xxs), ("xs", IconSize.xs), ("xsPlus", IconSize.xsPlus), ("s", IconSize.s),
        ("sPlus", IconSize.sPlus), ("m", IconSize.m), ("mPlus", IconSize.mPlus), ("l", IconSize.l),
        ("xl", IconSize.xl), ("xxl", IconSize.xxl),
      ]
    ),
    (
      "ControlHeight",
      [
        ("xs", ControlHeight.xs), ("xsPlus", ControlHeight.xsPlus), ("s", ControlHeight.s),
        ("sPlus", ControlHeight.sPlus), ("m", ControlHeight.m), ("mPlus", ControlHeight.mPlus),
        ("l", ControlHeight.l), ("xl", ControlHeight.xl),
      ]
    ),
    (
      "Radius",
      [
        ("extraSmall", Radius.extraSmall), ("small", Radius.small), ("medium", Radius.medium),
        ("large", Radius.large), ("extraLarge", Radius.extraLarge),
      ]
    ),
  ]

  /// The size names, smallest first. A `Plus` name sits just above its step.
  private static let sizeNames = [
    "hairline", "half", "xxs", "xs", "s", "m", "l", "xl", "xxl", "xxxl",
    "extraSmall", "small", "medium", "large", "extraLarge",
  ]

  /// Where `name` falls among the size names: its step's place, and half a place more for a `Plus` step.
  private static func rank(_ name: String) -> Double? {
    let base = name.hasSuffix("Plus") ? String(name.dropLast("Plus".count)) : name
    guard let place = sizeNames.firstIndex(of: base) else { return nil }
    return Double(place) + (base == name ? 0 : 0.5)
  }

  @Test("every spacing step is a whole even point except the hairline")
  func spacingIsOnTheGrid() {
    for step in Self.spacing where step != Spacing.hairline {
      #expect(step.truncatingRemainder(dividingBy: 2) == 0, "\(step)")
    }
  }

  @Test("each size scale ascends in the order its names say, a Plus step 2 points above its own")
  func sizeScalesAscendByName() throws {
    for (scale, steps) in Self.sizeScales {
      let ranked = try steps.map { step in
        (rank: try #require(Self.rank(step.name), "\(scale).\(step.name) is a size name"), value: step.value)
      }
      let byName = ranked.sorted { $0.rank < $1.rank }
        .map(\.value)
      #expect(byName == byName.sorted(), "\(scale)")
      #expect(Set(byName).count == byName.count, "\(scale): no two steps alike")
      let values = Dictionary(uniqueKeysWithValues: steps.map { ($0.name, $0.value) })
      for (name, value) in values where name.hasSuffix("Plus") {
        let base = String(name.dropLast("Plus".count))
        #expect(values[base].map { $0 + 2 } == value, "\(scale).\(name)")
      }
    }
  }

  @Test("a glyph font is the system font at the size and weight it is given, the regular weight when none is")
  func glyphFontIsTheSystemFontAtItsSize() {
    let side = ControlHeight.mPlus / 2
    #expect(Typography.font(size: side, weight: .semibold) == Font.system(size: side, weight: .semibold))
    #expect(Typography.font(size: side) == Font.system(size: side))
    #expect(Typography.font(size: side) != Typography.font(size: side, weight: .bold))
  }

  @Test("the type scale descends in whole sizes")
  func typeScaleDescendsWithWholeSizes() {
    let sizes = TextStyle.allCases
      .map(\.pointSize)
    #expect(sizes == sizes.sorted(by: >))
    #expect(Set(sizes).count == sizes.count)
    #expect(sizes.allSatisfy { $0 == $0.rounded() })
  }

  @Test("strokes and opacities ascend with no two steps alike, and durations ascend as declared")
  func scalesAscend() {
    let scales: [(name: String, steps: [Double], distinct: Bool)] = [
      ("stroke", [Stroke.hairline, Stroke.regular, Stroke.emphasis, Stroke.thick].map(Double.init), true),
      (
        "opacity",
        [
          Opacity.tint, Opacity.tintStrong, Opacity.edgeTintQuiet, Opacity.disabled, Opacity.dimmed, Opacity.muted,
          Opacity.edgeTint, Opacity.notYet,
        ],
        true
      ),
      (
        "duration",
        [
          Motion.Duration.drawDelay, Motion.Duration.press, Motion.Duration.contentOut, Motion.Duration.swapOut,
          Motion.Duration.hoverIn, Motion.Duration.crossfadeOut, Motion.Duration.step, Motion.Duration.pageOut,
          Motion.Duration.pressRelease, Motion.Duration.crossfadeReduced,
          Motion.Duration.dismiss, Motion.Duration.followUp, Motion.Duration.swapIn, Motion.Duration.textSwap,
          Motion.Duration.pageIn, Motion.Duration.leave, Motion.Duration.fade, Motion.Duration.hoverOut,
          Motion.Duration.reduced, Motion.Duration.crossfade, Motion.Duration.dim,
          Motion.Duration.resize, Motion.Duration.noteDelay,
          Motion.Duration.draw, Motion.Duration.state, Motion.Duration.shake,
          Motion.Duration.appearDelay,
          Motion.Duration.glideSoft, Motion.Duration.leastWork, Motion.Duration.doneHold, Motion.Duration.pulse,
          Motion.Duration.spin, Motion.Duration.breathe, Motion.Duration.toast,
          Motion.Duration.trace,
        ]
        .map(\.seconds),
        // Steps of different roles may share a length, such as fade, hoverOut and reduced.
        false
      ),
    ]
    for (name, steps, distinct) in scales {
      #expect(steps == steps.sorted(), "\(name)")
      #expect(!distinct || Set(steps).count == steps.count, "\(name)")
    }
  }

  @Test("elevations float higher in order, their radius and drop ascending and no two alike")
  func elevationsAscend() {
    let levels = Elevation.allCases
    #expect(levels.map(\.radius) == levels.map(\.radius).sorted())
    #expect(Set(levels.map(\.radius)).count == levels.count)
    #expect(levels.map(\.y) == levels.map(\.y).sorted())
  }

  @Test("every shadow is drawn in one of the shadow colours")
  func shadowsUseTheShadowColours() {
    for level in Elevation.allCases {
      #expect(
        [Color.dropShadowSelection, Color.dropShadowSoft, Color.dropShadow, Color.dropShadowStrong]
          .contains(level.color),
        "\(level)"
      )
    }
  }

  @Test("every radius, icon size and shadow radius is a whole even point")
  func stepsAreEven() {
    let shadows = Elevation.allCases
      .map(\.radius)
    let steps =
      [Radius.extraSmall, Radius.small, Radius.medium, Radius.large, Radius.extraLarge, Radius.window]
      + [IconSize.xxs, IconSize.xs, IconSize.xsPlus, IconSize.s, IconSize.sPlus, IconSize.m]
      + [IconSize.mPlus, IconSize.l, IconSize.xl, IconSize.xlPlus, IconSize.xxl]
      + shadows
    for step in steps {
      #expect(step.truncatingRemainder(dividingBy: 2) == 0, "\(step)")
    }
  }

  @Test("every colour wash is its base colour at the documented opacity, in the dark")
  func washesAreTheirBaseAtTheirOpacity() {
    let washes: [(name: String, wash: Color, base: Color, opacity: Float)] = [
      ("accentWash", .accentWash, .accent, 0.09),
      ("accentTint", .accentTint, .accent, 0.07),
      ("accentRing", .accentRing, .accent, 0.2),
      ("attentionWash", .attentionWash, .attention, 0.09),
      ("attentionFill", .attentionFill, .attention, 0.16),
      ("accentFill", .accentFill, .accent, 0.16),
      ("fillGroup", .fillGroup, .white, 0.05),
      ("fillRowHover", .fillRowHover, .white, 0.035),
      ("fillTile", .fillTile, .white, 0.045),
      ("fillTileRaised", .fillTileRaised, .white, 0.1),
      ("fillControlHover", .fillControlHover, .white, 0.12),
      ("strokeHighlight", .strokeHighlight, .white, 0.12),
      ("dropShadowSoft", .dropShadowSoft, .black, 0.4),
    ]
    let environment = EnvironmentValues.dark
    for (name, wash, base, opacity) in washes {
      let washed = wash.resolve(in: environment)
      let solid = base.resolve(in: environment)
      #expect(abs(washed.opacity - opacity) < 0.001, "\(name)")
      #expect(abs(washed.red - solid.red) < 0.001, "\(name)")
      #expect(abs(washed.green - solid.green) < 0.001, "\(name)")
      #expect(abs(washed.blue - solid.blue) < 0.001, "\(name)")
    }
  }

  @Test("a nested corner is the outer corner less its inset, and never below square")
  func nestedCornerIsTheOuterLessTheInset() {
    #expect(Radius.nested(in: Radius.window, inset: Spacing.s) == Radius.small)
    #expect(Radius.nested(in: Radius.extraLarge, inset: Spacing.m) == Spacing.mPlus)
    #expect(Radius.nested(in: Radius.small, inset: Spacing.l) == .zero)
  }

  @Test("a selection's shadow shows under Light and none under Dark")
  func selectionShadowIsLightOnly() {
    #expect(Elevation.selection.color.resolve(in: .dark).opacity == 0)
    #expect(Elevation.selection.color.resolve(in: .light).opacity > 0)
  }

  @Test("a style's line height is never less than its point size")
  func lineHeightsHoldTheirText() {
    for style in TextStyle.allCases {
      #expect(style.lineSpacing >= 0, "\(style)")
      guard let lineHeight = style.lineHeight else {
        #expect(style.lineSpacing == 0, "\(style) keeps the font's own height")
        continue
      }
      #expect(lineHeight >= style.pointSize, "\(style)")
    }
  }

  @Test("every new solid colour is its documented value, in the dark")
  func newColoursHaveTheirValues() {
    let colours: [(name: String, colour: Color, rgb: [Float], opacity: Float)] = [
      ("fillPrimaryHover", .fillPrimaryHover, [0xFF, 0xFF, 0xFF], 1),
      ("fillPrimaryPressed", .fillPrimaryPressed, [0xE3, 0xE3, 0xE8], 1),
      ("fillModal", .fillModal, [38, 38, 42], 1),
      ("fillFloating", .fillFloating, [40, 40, 44], 1),
      ("fillOverPicture", .fillOverPicture, [20, 20, 22], 1),
      ("accentHover", .accentHover, [0x1A, 0x8C, 0xFF], 1),
      ("onAttention", .onAttention, [0x1C, 0x12, 0x03], 1),
      ("fillSelection", .fillSelection, [0xFF, 0xFF, 0xFF], 0.12),
      ("strokeSelected", .strokeSelected, [0xFF, 0xFF, 0xFF], 0.06),
    ]
    let environment = EnvironmentValues.dark
    for (name, colour, rgb, opacity) in colours {
      let resolved = colour.resolve(in: environment)
      let channels = [resolved.red, resolved.green, resolved.blue]
      for (channel, value) in zip(channels, rgb) {
        #expect(abs(channel * 255 - value) < 0.5, "\(name)")
      }
      #expect(abs(resolved.opacity - opacity) < 0.001, "\(name)")
    }
  }
}
