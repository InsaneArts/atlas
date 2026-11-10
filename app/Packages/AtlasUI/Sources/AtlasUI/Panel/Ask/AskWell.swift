import SwiftUI

/// What a question shows of what would be sent, in a well inside its card: the message under its label, a picture
/// of the element ringed in blue, or a line in place of either.
struct AskWell: View {
  /// What it shows.
  let content: AskCardBody

  /// The well.
  var body: some View {
    switch content {
    case .message(let text):
      VStack(alignment: .leading, spacing: AskCardLayout.wellGap) {
        Text("Message")
          .font(Typography.font(.caption, weight: .semibold))
          .foregroundStyle(Color.textTertiary)
        Text(text)
          .textStyle(.body)
          .foregroundStyle(Color.textPrimary)
          .lineLimit(AskCardLayout.messageLines)
          .truncationMode(.tail)
      }
      .frame(maxWidth: .infinity, alignment: .leading)
      .padding(AskCardLayout.wellInset)
      .background(well.fill(Color.fillTile))
      .accessibilityElement(children: .combine)
    case .picture(let picture, let ring):
      MarkedPicture(picture: picture, mark: ViewerActionMark(shape: .press, area: ring))
        .frame(maxWidth: .infinity)
        .frame(height: AskCardLayout.pictureHeight)
        .clipShape(well)
        .accessibilityLabel(Text(verbatim: "A picture of what it would press, ringed in blue"))
    case .note(let text):
      Text(text)
        .textStyle(.callout)
        .foregroundStyle(Color.textSecondary)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(AskCardLayout.wellInset)
        .background(well.fill(Color.fillTile))
    }
  }

  /// The well's outline.
  private var well: RoundedRectangle {
    RoundedRectangle(cornerRadius: AskCardLayout.wellCorner, style: .continuous)
  }
}
