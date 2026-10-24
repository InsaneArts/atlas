import SwiftUI

/// A labelled text field in a card. Flagged, it shows what the note under the form is about: its label in the
/// attention colour, a warning mark at its end, and an attention edge around it.
struct FieldRow: View {
  /// The field's name, on the left.
  let label: String
  /// What is typed.
  @Binding var text: String
  /// Whether the text is hidden, as a password's is.
  var secure = false
  /// Whether the problem under the form is about this field.
  var isFlagged = false

  var body: some View {
    HStack(spacing: Spacing.m) {
      Text(label)
        .font(Typography.font(.body))
        .foregroundStyle(isFlagged ? Color.textAttention : Color.textSecondary)
        .frame(width: Layout.labelWidth, alignment: .leading)
      Group {
        if secure {
          SecureField("", text: $text)
        } else {
          TextField("", text: $text)
        }
      }
      .textFieldStyle(.plain)
      .font(Typography.font(.body))
      .accessibilityLabel(label)
      if isFlagged {
        Image(systemName: "exclamationmark.triangle.fill")
          .font(Typography.font(.callout))
          .foregroundStyle(Color.attention)
          .accessibilityHidden(true)
      }
    }
    .padding(.horizontal, Spacing.mPlus)
    .frame(height: ControlHeight.l)
    .background {
      if isFlagged {
        RoundedRectangle(cornerRadius: Radius.small)
          .fill(
            Color.attention
              .opacity(Opacity.tint)
          )
          .overlay(
            RoundedRectangle(cornerRadius: Radius.small)
              .strokeBorder(Color.attention, lineWidth: Stroke.regular)
          )
          .padding(Spacing.half)
      }
    }
  }
}

/// The row's own measures.
private enum Layout {
  /// 120: the labels' column, wide enough for "Account name" so the fields line up.
  static let labelWidth: CGFloat = 120
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.field)
  }
#endif
