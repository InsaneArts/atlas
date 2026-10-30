import SwiftUI

/// A screenshot's thumbnail wherever one shows, the Screenshots grid and an Activity step alike: the whole screenshot
/// at its own shape (``ShotShape``), never cut or zoomed, as large as the room it is given holds and standing at its
/// `alignment` in it, with ``PictureTile``'s edge, its brighter edge under the pointer and its one selection ring
/// outside it. Where a zoom is given, its picture is the one that zooms between here and the large view.
struct ShotThumbnail: View {
  /// The thumbnail; nil until it is read.
  let picture: DesktopPicture?
  /// The screenshot's width over its height (``ShotShape/of(_:listed:)``).
  let shape: CGFloat
  /// Its corners.
  var radius = Radius.medium
  /// Whether it is the one selected, which its ring shows.
  var isSelected = false
  /// Whether the pointer is over it, which a brighter edge shows.
  var isHighlighted = false
  /// Whether it draws its fine edge.
  var showsEdge = true
  /// What shows until the thumbnail arrives.
  var ground = Color.letterbox
  /// Where it stands in the room it is given, when that room is not its shape.
  var alignment = Alignment.bottomLeading
  /// The screenshot's file name and the window's namespace its picture zooms in; nil zooms nothing.
  var zoom: (id: String, namespace: Namespace.ID)?

  /// The picture at its shape, standing in its room.
  var body: some View {
    tile
      .aspectRatio(shape, contentMode: .fit)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: alignment)
  }

  /// The picture, carrying the zoom where it has one.
  @ViewBuilder private var tile: some View {
    let picture = PictureTile(
      picture: picture,
      radius: radius,
      isSelected: isSelected,
      isHighlighted: isHighlighted,
      showsEdge: showsEdge,
      ground: ground
    )
    if let zoom {
      picture
        .shotZoom(id: zoom.id, in: zoom.namespace)
    } else {
      picture
    }
  }
}

#if DEBUG
  #Preview("Samples") {
    HStack(spacing: Spacing.l) {
      ShotThumbnail(picture: SampleDesktop.picture, shape: ShotShape.standard, isSelected: true)
      ShotThumbnail(picture: SampleDesktop.picture, shape: ShotShape.of(SampleDesktop.picture, listed: nil))
      ShotThumbnail(picture: nil, shape: ShotShape.standard)
    }
    .frame(width: SampleWidth.column, height: SampleWidth.narrow / ShotShape.standard)
    .padding(Spacing.s)
    .sampleSurface()
  }
#endif
