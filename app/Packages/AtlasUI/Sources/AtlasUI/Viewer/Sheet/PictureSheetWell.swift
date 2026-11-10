import SwiftUI

/// The sheet's picture well: 16:10 on the recessed ground with a hairline rim inside, its corner nested in the sheet's,
/// the picture fitted 24 inside it with a soft shadow, and a step's mark over the screen before it.
struct PictureSheetWell: View {
  /// The sheet.
  let sheet: ViewerPictureSheet
  @Environment(\.motionReduced) private var reduceMotion

  /// The well and the picture.
  var body: some View {
    let shape = RoundedRectangle(
      cornerRadius: Radius.nested(in: ViewerLayout.sheetRadius, inset: ViewerLayout.sheetInset),
      style: .continuous
    )
    Color.groundRecessed
      .aspectRatio(ViewerLayout.placeholderAspect, contentMode: .fit)
      .overlay {
        GeometryReader { proxy in
          let room = CGSize(
            width: proxy.size.width - ViewerLayout.sheetPictureInset * 2,
            height: proxy.size.height - ViewerLayout.sheetPictureInset * 2
          )
          let size = ViewerLayout.fitted(sheet.aspect, in: room)
          MarkedPicture(picture: sheet.picture, mark: sheet.shownSide == .before ? sheet.mark : nil)
            .frame(width: size.width, height: size.height)
            .elevation(.resting)
            .frame(width: proxy.size.width, height: proxy.size.height)
            .id(sheet.id)
            .transition(.opacity)
        }
      }
      .animation(Motion.animation(Motion.fade, reduceMotion: reduceMotion), value: sheet.picture)
      .clipShape(shape)
      .overlay(shape.strokeBorder(Color.strokeContent, lineWidth: Stroke.hairline))
      .accessibilityElement()
      .accessibilityLabel(Text(verbatim: sheet.line))
      .accessibilityAddTraits(.isImage)
  }
}
