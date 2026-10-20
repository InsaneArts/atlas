import CoreGraphics
import Testing

@testable import AtlasUI

@Suite("Design system")
struct DesignSystemTests {
  /// The spacing scale, smallest first.
  private static let spacing: [CGFloat] = [
    Spacing.hairline, Spacing.xxs, Spacing.xs, Spacing.s, Spacing.m, Spacing.ml, Spacing.l, Spacing.xl, Spacing.xxl,
    Spacing.group, Spacing.section, Spacing.page,
  ]

  @Test("every spacing step but the hairline is on the 2-point grid, and the scale ascends")
  func spacingIsOnTheGridAndAscends() {
    #expect(Self.spacing == Self.spacing.sorted())
    #expect(Set(Self.spacing).count == Self.spacing.count)
    for step in Self.spacing where step != Spacing.hairline {
      #expect(step.truncatingRemainder(dividingBy: 2) == 0, "\(step)")
    }
  }

  @Test("the type scale descends in whole sizes")
  func typeScaleDescendsWithWholeSizes() {
    let sizes = TextStyle.allCases
      .map(\.pointSize)
    #expect(sizes == sizes.sorted(by: >))
    #expect(Set(sizes).count == sizes.count)
    #expect(sizes.allSatisfy { $0 == $0.rounded() })
  }

  @Test("radii, strokes, icon sizes, control heights and durations ascend")
  func scalesAscend() {
    let scales: [(String, [Double])] = [
      ("radius", [Radius.small, Radius.medium, Radius.large].map(Double.init)),
      ("stroke", [Stroke.hairline, Stroke.regular, Stroke.emphasis, Stroke.thick].map(Double.init)),
      (
        "icon size",
        [
          IconSize.dot, IconSize.glyph, IconSize.glyphLarge, IconSize.radio, IconSize.mark, IconSize.disc,
          IconSize.tile, IconSize.avatar, IconSize.avatarLarge, IconSize.hero,
        ]
        .map(Double.init)
      ),
      (
        "control height",
        [
          ControlHeight.badge, ControlHeight.pill, ControlHeight.chip, ControlHeight.menuRow, ControlHeight.row,
          ControlHeight.compactRow, ControlHeight.button, ControlHeight.field, ControlHeight.large,
        ]
        .map(Double.init)
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
}
