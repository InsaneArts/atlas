import CoreGraphics
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("Design system")
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

  @Test("strokes, opacities and durations ascend, no two steps alike")
  func scalesAscend() {
    let scales: [(String, [Double])] = [
      ("stroke", [Stroke.hairline, Stroke.regular, Stroke.emphasis, Stroke.thick].map(Double.init)),
      (
        "opacity",
        [
          Opacity.tint, Opacity.tintStrong, Opacity.edgeTintQuiet, Opacity.disabled, Opacity.dimmed, Opacity.muted,
          Opacity.edgeTint, Opacity.demonstration,
        ]
      ),
      (
        "duration",
        [
          Motion.Duration.press, Motion.Duration.fade, Motion.Duration.entry, Motion.Duration.glide,
          Motion.Duration.pulse, Motion.Duration.spin,
        ]
        .map(\.seconds)
      ),
    ]
    for (name, steps) in scales {
      #expect(steps == steps.sorted(), "\(name)")
      #expect(Set(steps).count == steps.count, "\(name)")
    }
  }

  @Test("elevations float higher in order, their radius and drop ascending and no two alike")
  func elevationsAscend() {
    let levels = Elevation.allCases
    #expect(levels.map(\.radius) == levels.map(\.radius).sorted())
    #expect(Set(levels.map(\.radius)).count == levels.count)
    #expect(levels.map(\.y) == levels.map(\.y).sorted())
  }

  @Test("every shadow is drawn in one of the two shadow colours")
  func shadowsUseTheShadowColours() {
    for level in Elevation.allCases {
      #expect([Color.dropShadow, Color.dropShadowStrong].contains(level.color), "\(level)")
    }
  }

  @Test("every radius, icon size and shadow radius is a whole even point")
  func stepsAreEven() {
    let shadows = Elevation.allCases
      .map(\.radius)
    let steps =
      [Radius.extraSmall, Radius.small, Radius.medium, Radius.large, Radius.extraLarge]
      + [IconSize.xxs, IconSize.xs, IconSize.xsPlus, IconSize.s, IconSize.sPlus, IconSize.m]
      + [IconSize.mPlus, IconSize.l, IconSize.xl, IconSize.xxl]
      + shadows
    for step in steps {
      #expect(step.truncatingRemainder(dividingBy: 2) == 0, "\(step)")
    }
  }
}
