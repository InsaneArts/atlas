import SwiftUI

/// One row of a details group, 32 high, its container giving its side inset: a quiet name on the left, its value on
/// the right, and, for a readiness check, its mark first and what it means under the name while it is not ready.
struct DetailRow: View {
  /// The name, such as "Account" or "Mouse and keyboard".
  let name: String
  /// The value, cut short with its whole value as a tooltip; empty for none.
  var value = ""
  /// How the check stands, which its mark shows; nil draws no mark.
  var readiness: ReadinessState?
  /// What a check that is not ready means, under its name; nil for none.
  var note: String?
  /// Whether the name is quiet, as a count of checks that pass is.
  var isQuiet = false

  var body: some View {
    HStack(alignment: .firstTextBaseline, spacing: Spacing.m) {
      if let readiness {
        ReadinessMark(state: readiness)
          .alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + Layout.markBaseline }
      }
      VStack(alignment: .leading, spacing: Spacing.xxs) {
        Text(name)
          .foregroundStyle(isQuiet ? Color.textTertiary : Color.textSecondary)
        if let note {
          Text(note)
            .font(Typography.font(.callout))
            .foregroundStyle(readiness?.noteColor ?? Color.textTertiary)
            .fixedSize(horizontal: false, vertical: true)
        }
      }
      Spacer(minLength: Spacing.m)
      Text(value)
        .lineLimit(1)
        .truncationMode(.tail)
        .help(value)
    }
    .font(Typography.font(.body))
    .padding(.vertical, note == nil ? .zero : Spacing.s)
    .frame(minHeight: ControlHeight.compactRow)
    .accessibilityElement(children: .combine)
    .accessibilityValue(readiness?.spokenValue ?? "")
  }
}

/// The row's own measures, from the board (mac.py `details_sidebar`).
private enum Layout {
  /// 4: how far below the mark's centre the name's baseline sits, so a 13-point mark centres on a 13-point name.
  static let markBaseline: CGFloat = 4
}

#if DEBUG
  #Preview("Samples") {
    RowSamples(.detail)
  }
#endif
