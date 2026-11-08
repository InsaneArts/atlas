import SwiftUI

/// The picture sheet itself, in the app's own light or dark colours: its title row, the picture well and the footer,
/// on the modal ground with a strong hairline rim, 824 wide on a wide window so its well is 800 × 500.
struct PictureSheetCard: View {
  /// The sheet.
  let sheet: ViewerPictureSheet
  /// The window's size, which a narrow sheet gives way to.
  let window: CGSize
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// The header, the well and the footer.
  var body: some View {
    let width = ViewerLayout.sheetWidth(window: window.width)
    let shape = RoundedRectangle(cornerRadius: ViewerLayout.sheetRadius, style: .continuous)
    VStack(spacing: .zero) {
      PictureSheetHeader(sheet: sheet, send: send)
      PictureSheetWell(sheet: sheet)
        .padding(.horizontal, ViewerLayout.sheetInset)
      PictureSheetFooter(sheet: sheet, window: window.width, send: send)
    }
    .frame(width: width)
    .background(shape.fill(Color.fillModal))
    .overlay(shape.strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline))
    .clipShape(shape)
    .elevation(.modal)
    .accessibilityElement(children: .contain)
    .accessibilityLabel(Text(verbatim: sheet.title.text))
  }
}
