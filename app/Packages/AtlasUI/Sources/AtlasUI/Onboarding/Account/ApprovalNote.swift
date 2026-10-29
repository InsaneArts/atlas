import SwiftUI

/// Why macOS asks for a password, and what Atlas does with the approval, shown before macOS's own dialog, whose words
/// are generic: a quiet card under the form, its title and three lines.
struct ApprovalNote: View {
  /// Who may add an account, what Atlas does with the approval, and what it keeps.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.xsPlus) {
      HStack(spacing: Layout.symbolGap) {
        Image(systemName: SymbolName.lock.rawValue)
          .font(Typography.font(size: StatusLine.markSide))
          .foregroundStyle(Color.textSecondary)
          .accessibilityHidden(true)
        Text("Why macOS asks for your password")
          .textStyle(.callout, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
      }
      .accessibilityElement(children: .combine)
      .accessibilityAddTraits(.isHeader)
      Text(
        "Only an administrator can add an account. Atlas uses your approval once to create a standard account, skip its welcome screens and open Atlas there. It keeps neither the approval nor the password."
      )
    }
    .textStyle(.caption)
    .foregroundStyle(Color.textSecondary)
    .fixedSize(horizontal: false, vertical: true)
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(.horizontal, Spacing.mPlus)
    .padding(.vertical, Spacing.m)
    .background(
      RoundedRectangle(cornerRadius: Radius.large, style: .continuous)
        .filledOutline(Color.fillQuiet, border: Color.separator, lineWidth: Stroke.hairline)
    )
  }
}

/// The approval card's own measures.
private enum Layout {
  /// 7: between the lock and the title.
  static let symbolGap: CGFloat = 7
}

#if DEBUG
  #Preview("Approval") {
    ApprovalNote()
      .frame(width: OnboardingLayout.column)
      .onboardingSample()
  }
#endif
