import SwiftUI

/// The callout of a step that did not work: what happened, then why and what came next under it, on a soft orange
/// capsule for a failure or a quiet grey one for an action refused before anything was sent. Its corner is a 28-point
/// capsule's, so a one-line callout is a true capsule and a taller one keeps the same ends. The action whose outcome is
/// unknown also offers Show Screen and Recover….
struct ActivityProblemCallout: View {
  /// The step.
  let step: ViewerActivityStep
  /// Receives what the person does.
  let send: @MainActor (ViewerEvent) -> Void

  /// What happened, why and what came next, on its capsule.
  var body: some View {
    VStack(alignment: .leading, spacing: ViewerLayout.calloutGap) {
      Text(verbatim: step.words.text)
        .font(Typography.font(.body, weight: .medium))
        .foregroundStyle(Color.textPrimary)
      if let detail = step.problem?.detail, !detail.isEmpty {
        Text(verbatim: detail)
          .font(Typography.font(.callout))
          .foregroundStyle(Color.textSecondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      if step.offersRecovery {
        ActivityLineRecovery(send: send)
          .padding(.top, Spacing.xs)
      }
    }
    .padding(.vertical, ViewerLayout.stepInset)
    .padding(.horizontal, ViewerLayout.calloutInset)
    .background(
      RoundedRectangle(cornerRadius: ViewerLayout.calloutRadius, style: .continuous)
        .fill(step.mark == .notSent ? Color.fillControl : Color.fillAttentionSoft)
    )
  }
}
