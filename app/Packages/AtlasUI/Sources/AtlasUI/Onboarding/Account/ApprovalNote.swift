import SwiftUI

/// Why Atlas asks for administrator access once, and exactly what it does with it, shown before macOS's own dialog,
/// whose words are generic: a quiet card under the form, its three steps a numbered list (v3 `.approval`).
struct ApprovalNote: View {
  /// What the approval is for, the steps Atlas takes with it, and what it does not do.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.xsPlus) {
      HStack(spacing: Layout.symbolGap) {
        Image(systemName: SymbolName.lock.rawValue)
          .font(Typography.font(size: StatusLine.markSide))
          .foregroundStyle(Color.textSecondary)
          .accessibilityHidden(true)
        Text("Why Atlas asks for administrator access")
          .textStyle(.callout, weight: .semibold)
          .foregroundStyle(Color.textPrimary)
      }
      .accessibilityElement(children: .combine)
      .accessibilityAddTraits(.isHeader)
      Text("Only an administrator can create an account on this Mac. You approve once, and Atlas:")
      VStack(alignment: .leading, spacing: Spacing.xs) {
        step(
          1,
          "creates a standard account, not an administrator, with this name and password, which you log in with after each restart"
        )
        step(2, "answers macOS's welcome questions in it, so you go straight to its desktop")
        step(3, "adds a one-time item that opens Atlas there, which then removes itself")
      }
      Text(
        "Nothing else on your Mac changes. Atlas asks only now, during setup, and keeps neither the approval nor the password."
      )
      .foregroundStyle(Color.textTertiary)
    }
    .textStyle(.caption)
    .foregroundStyle(Color.textSecondary)
    .fixedSize(horizontal: false, vertical: true)
    .frame(maxWidth: .infinity, alignment: .leading)
    .padding(.horizontal, Spacing.mPlus)
    .padding(.vertical, Spacing.m)
    .background(
      RoundedRectangle(cornerRadius: Radius.large)
        .filledOutline(Color.fillQuiet, border: Color.separator, lineWidth: Stroke.hairline)
    )
  }

  /// One numbered step: its number on a small disc, then what Atlas does, read as one line.
  ///
  /// - Parameters:
  ///   - number: The step's number.
  ///   - text: What Atlas does.
  /// - Returns: The step.
  private func step(_ number: Int, _ text: LocalizedStringKey) -> some View {
    HStack(alignment: .top, spacing: Layout.symbolGap) {
      Text(verbatim: "\(number)")
        .textStyle(.footnote, weight: .semibold)
        .foregroundStyle(Color.textPrimary)
        .frame(width: IconSize.s, height: IconSize.s)
        .background(
          Circle()
            .fill(Color.fillControl)
        )
      Text(text)
    }
    .accessibilityElement(children: .combine)
  }
}

/// v3's approval card (`.approval`).
private enum Layout {
  /// 7: between the lock and the title, and between a step's number and its words.
  static let symbolGap: CGFloat = 7
}

#if DEBUG
  #Preview("Approval") {
    ApprovalNote()
      .frame(width: OnboardingLayout.column)
      .onboardingSample()
  }
#endif
