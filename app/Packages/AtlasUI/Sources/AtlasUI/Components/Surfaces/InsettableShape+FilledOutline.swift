import SwiftUI

extension InsettableShape {
  /// The shape filled and bordered inside its edge: the one drawing behind every bordered surface, capsule and
  /// circle (U-9), in place of a fill and a hairline written out by hand. The border is laid over the fill, as the
  /// hand-written pairs lay it, so a pair moved here draws the same pixels.
  ///
  /// - Parameters:
  ///   - fill: The fill: a colour, a gradient or another shape style.
  ///   - border: The border's paint.
  ///   - lineWidth: The border's width; a hairline by default.
  /// - Returns: The filled, bordered shape.
  func filledOutline(
    _ fill: some ShapeStyle,
    border: some ShapeStyle,
    lineWidth: CGFloat = Stroke.hairline
  ) -> some View {
    self.fill(fill)
      .overlay(strokeBorder(border, lineWidth: lineWidth))
  }
}

#if DEBUG
  #Preview("Shapes") {
    HStack(spacing: Spacing.l) {
      RoundedRectangle(cornerRadius: Radius.large)
        .filledOutline(Color.fillGroup, border: Color.separator)
        .frame(width: IconSize.xxl, height: IconSize.xxl)
      Capsule()
        .filledOutline(Color.fillControl, border: Color.strokeStrong)
        .frame(width: IconSize.xxl, height: ControlHeight.sPlus)
      Circle()
        .filledOutline(Color.fillControlOn, border: Color.strokePressed, lineWidth: Stroke.regular)
        .frame(width: ControlHeight.sPlus, height: ControlHeight.sPlus)
    }
    .sampleSurface()
  }
#endif
