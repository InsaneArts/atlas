import SwiftUI

/// The helper's name over why it is in its mode, at the top of the sidebar, read as one.
struct SidebarAccount: View {
  /// The helper as people know it.
  let account: String
  /// Why the helper is in its mode.
  let reason: String

  /// The eyebrow, the name and the reason.
  var body: some View {
    VStack(alignment: .leading, spacing: Spacing.xs) {
      Eyebrow(text: "Helper")
      Text(account)
        .font(Typography.font(.title3, weight: .semibold))
        .tracking(LetterSpacing.title)
      Text(reason)
        .font(Typography.font(.label))
        .foregroundStyle(Color.textSecondary)
        .fixedSize(horizontal: false, vertical: true)
    }
    .accessibilityElement(children: .combine)
  }
}

#if DEBUG
  #Preview("Watching") {
    SidebarAccount(
      account: PreviewStates.viewerSidebar.sidebar.account,
      reason: PreviewStates.viewerSidebar.sidebar.reason
    )
    .frame(width: SampleWidth.narrow, alignment: .leading)
    .sampleSurface()
  }
#endif
