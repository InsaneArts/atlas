import SwiftUI

/// What Atlas knows of an action shown large, beside its picture: one line after another, each named quietly above
/// its value, the outcome in its colour.
struct ActionDetailsPanel: View {
  /// The lines.
  let details: [ViewerActionDetail]

  /// The lines on a dark panel.
  var body: some View {
    VStack(alignment: .leading, spacing: .zero) {
      ForEach(Array(details.enumerated()), id: \.element.id) { index, detail in
        VStack(alignment: .leading, spacing: Stroke.regular) {
          Text(verbatim: detail.title)
            .font(Typography.font(.caption))
            .foregroundStyle(Color.markBubbleDim)
          Text(verbatim: detail.value)
            .font(Typography.font(.callout, weight: .medium))
            .foregroundStyle(detail.tone?.textColor ?? Color.textOverPicture)
            .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, Spacing.xsPlus)
        .overlay(alignment: .top) {
          if index > 0 {
            Rectangle()
              .fill(Color.overPictureLine)
              .frame(height: Stroke.hairline)
          }
        }
        .accessibilityElement(children: .combine)
      }
    }
    .padding(.horizontal, Spacing.mPlus)
    .padding(.vertical, Spacing.s)
    .frame(width: ViewerLayout.actionDetails)
    .background(
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .fill(Color.fillOverPicture)
    )
  }
}
