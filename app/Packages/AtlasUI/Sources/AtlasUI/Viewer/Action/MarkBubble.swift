import SwiftUI

/// What was typed, in a dark bubble over the field: at most three lines, then a quiet line such as "Typed · 70
/// characters", or eight dots and "A password"; the quiet line alone when the text was not kept.
struct MarkBubble: View {
  /// What was typed, or eight dots.
  let text: String
  /// The quiet line under it.
  let detail: String
  /// The widest it stands.
  let width: CGFloat

  /// The words and the quiet line: on one line while they fit, else wrapped at the widest it stands.
  var body: some View {
    ViewThatFits(in: .horizontal) {
      words
        .padding(.horizontal, Spacing.sPlus)
        .fixedSize()
      words
        .padding(.horizontal, Spacing.sPlus)
        .frame(width: width, alignment: .leading)
        .fixedSize(horizontal: false, vertical: true)
    }
    .padding(.vertical, Spacing.s)
    .background(
      RoundedRectangle(cornerRadius: Radius.medium, style: .continuous)
        .fill(Color.markBubble)
    )
    .shadow(color: Color.dropShadow, radius: Spacing.xsPlus, y: Stroke.regular)
  }

  /// What was typed over the quiet line.
  private var words: some View {
    VStack(alignment: .leading, spacing: Spacing.half) {
      if !text.isEmpty {
        Text(verbatim: text)
          .font(Typography.font(.callout))
          .foregroundStyle(Color.markBubbleText)
          .lineLimit(ViewerLayout.markBubbleLines)
          .truncationMode(.tail)
      }
      Text(verbatim: detail)
        .font(Typography.font(.caption))
        .foregroundStyle(Color.markBubbleDim)
    }
  }
}
