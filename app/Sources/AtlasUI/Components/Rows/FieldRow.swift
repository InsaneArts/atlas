import SwiftUI

/// A labelled text field in a card.
struct FieldRow: View {
  /// The field's name, on the left.
  let label: String
  /// What is typed.
  @Binding var text: String
  /// Whether the text is hidden, as a password's is.
  var secure = false

  var body: some View {
    HStack(spacing: Spacing.l) {
      Text(label)
        .font(Typography.font(.body))
        .foregroundStyle(Color.textSecondary)
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
    }
    .padding(.horizontal, Spacing.xl)
    .frame(height: ControlHeight.field)
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
