import SwiftUI

/// Why Atlas asks for administrator access once, and exactly what it does with it, shown before macOS's own dialog,
/// whose words are generic.
struct ApprovalNote: View {
  /// What the approval is for, the steps Atlas takes with it, and what it does not do.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.xs) {
      Text("Why Atlas asks for administrator access")
        .font(Typography.font(.callout, weight: .semibold))
        .foregroundStyle(Color.textPrimary)
      Text("Only an administrator can create an account on this Mac. You approve once, and Atlas:")
      Text(
        "1. creates a standard account, not an administrator, with this name and password, which you log in with after each restart"
      )
      Text("2. answers macOS's welcome questions in it, so you go straight to its desktop")
      Text("3. adds a one-time item that opens Atlas there, which then removes itself")
      Text(
        "Nothing else on your Mac changes. Atlas asks only now, during setup, and keeps neither the approval nor the password."
      )
    }
    .font(Typography.font(.caption))
    .foregroundStyle(Color.textSecondary)
    .fixedSize(horizontal: false, vertical: true)
    .frame(maxWidth: .infinity, alignment: .leading)
  }
}

#if DEBUG
  #Preview("Approval") {
    ApprovalNote()
      .frame(width: OnboardingLayout.column)
      .onboardingSample()
  }
#endif
