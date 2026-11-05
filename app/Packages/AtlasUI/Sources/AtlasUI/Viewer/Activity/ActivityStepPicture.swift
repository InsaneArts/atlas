import SwiftUI

/// A step's small picture, at the end of its line: the screen just before it, cropped and zoomed in around the element
/// it went to or the spot so the mark still reads at this size, the element whole and in the middle unless that would
/// show past the picture's edge; the
/// whole screen where no spot is known, or where it is asked not to zoom, as a replay's strip shows a whole frame.
/// Repeats folded into one step stand on a small stack with their count.
struct ActivityStepPicture: View {
  /// The picture and its mark.
  let picture: ViewerStepPicture
  /// Whether it zooms in around the spot.
  var zooms = true
  /// Its width; its height keeps ``ViewerLayout/stepPictureShape``.
  var width = ViewerLayout.stepPictureWidth

  /// The stack, the crop and its mark, and the count.
  var body: some View {
    let size = CGSize(width: width, height: width / ViewerLayout.stepPictureShape)
    crop(size)
      .frame(width: size.width, height: size.height)
      .clipShape(RoundedRectangle(cornerRadius: ViewerLayout.stepPictureRadius, style: .continuous))
      .overlay {
        RoundedRectangle(cornerRadius: ViewerLayout.stepPictureRadius, style: .continuous)
          .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
      }
      .background { stack(size) }
      .overlay(alignment: .bottomTrailing) { count }
  }

  /// The picture zoomed around its focus, and the mark over it at its own size.
  ///
  /// - Parameter size: The crop's size.
  /// - Returns: The crop.
  private func crop(_ size: CGSize) -> some View {
    let frame = Self.frame(
      aspect: picture.shot.thumbnail?.aspectRatio ?? picture.shot.aspect,
      focus: zooms ? picture.mark?.focus : nil,
      area: zooms ? picture.mark?.area : nil,
      in: size
    )
    return Color.fillControl
      .overlay(alignment: .topLeading) {
        if let image = picture.shot.thumbnail {
          Image(decorative: image.image, scale: 1)
            .resizable()
            .interpolation(.high)
            .frame(width: frame.width, height: frame.height)
            .offset(x: frame.minX, y: frame.minY)
        }
      }
      .overlay {
        if let mark = picture.mark, picture.shot.thumbnail != nil {
          ActionMarkLayer(mark: mark, style: .compact, frame: frame)
        }
      }
  }

  /// The edges of the pictures behind a folded step, up and to the side.
  ///
  /// - Parameter size: The crop's size.
  /// - Returns: The stack, or nothing for a step on its own.
  @ViewBuilder private func stack(_ size: CGSize) -> some View {
    if picture.repeats > 1 {
      ForEach(1...2, id: \.self) { depth in
        let offset = ViewerLayout.stepPictureStack * CGFloat(depth)
        RoundedRectangle(cornerRadius: ViewerLayout.stepPictureRadius, style: .continuous)
          .fill(Color.ground)
          .overlay {
            RoundedRectangle(cornerRadius: ViewerLayout.stepPictureRadius, style: .continuous)
              .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
          }
          .offset(x: offset, y: -offset)
          .zIndex(-Double(depth))
      }
    }
  }

  /// A folded step's count, in the mark's colour at the picture's corner.
  @ViewBuilder private var count: some View {
    if picture.repeats > 1 {
      Text(verbatim: "\(picture.repeats)")
        .font(Typography.digits(.caption2, weight: .bold))
        .foregroundStyle(Color.onMark)
        .padding(.horizontal, Spacing.xs)
        .frame(minWidth: ViewerLayout.stepCount, minHeight: ViewerLayout.stepCount)
        .background(Capsule().fill((picture.mark?.tone ?? .worked).color))
        .padding(Spacing.half)
    }
  }

  /// Where the whole picture stands so that `focus` sits in the middle of a crop `size` large, zoomed in by
  /// ``ViewerLayout/stepPictureZoom``, or less where `area` would not fit the crop then, and the crop never shows past
  /// the picture's edge; with no focus, the whole picture filling the crop.
  ///
  /// - Parameters:
  ///   - aspect: The picture's width over its height.
  ///   - focus: The spot, as a share of the picture; nil for none.
  ///   - area: The element to keep whole, as shares of the picture; none by default.
  ///   - size: The crop's size.
  /// - Returns: The picture's frame, in the crop's own space.
  nonisolated static func frame(aspect: CGFloat, focus: CGPoint?, area: CGRect? = nil, in size: CGSize) -> CGRect {
    let shape = aspect.isFinite && aspect > 0 ? aspect : ShotShape.standard
    let fill = max(size.width / shape, size.height)
    let fits =
      area.map {
        min(
          size.width / max($0.width * fill * shape, .ulpOfOne),
          size.height / max($0.height * fill, .ulpOfOne)
        )
      } ?? ViewerLayout.stepPictureZoom
    let zoom = focus == nil ? 1 : min(max(fits, 1), ViewerLayout.stepPictureZoom)
    let drawn = CGSize(width: fill * shape * zoom, height: fill * zoom)
    let spot = focus ?? CGPoint(x: 0.5, y: 0.5)
    let origin = CGPoint(
      x: min(0, max(size.width - drawn.width, size.width / 2 - spot.x * drawn.width)),
      y: min(0, max(size.height - drawn.height, size.height / 2 - spot.y * drawn.height))
    )
    return CGRect(origin: origin, size: drawn)
  }
}

#if DEBUG
  #Preview("Step pictures") {
    HStack(spacing: Spacing.l) {
      ForEach(PreviewStates.sampleStepPictures.indices, id: \.self) { index in
        ActivityStepPicture(picture: PreviewStates.sampleStepPictures[index])
      }
    }
    .padding(Spacing.s)
    .sampleSurface()
  }

  #Preview("Step pictures, dark") {
    HStack(spacing: Spacing.l) {
      ForEach(PreviewStates.sampleStepPictures.indices, id: \.self) { index in
        ActivityStepPicture(picture: PreviewStates.sampleStepPictures[index])
      }
    }
    .padding(Spacing.s)
    .sampleSurface()
    .environment(\.colorScheme, .dark)
  }
#endif
