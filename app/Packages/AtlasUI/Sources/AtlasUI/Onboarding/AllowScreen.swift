import SwiftUI

/// In the helper: whether to let the account that asked use this one through Atlas. Allow is the footer's.
struct AllowScreen: View {
  /// Who asked, and what allowing means.
  let state: OnboardingAllowScreenState
  /// The line under what allowing means.
  let note: WorkNote

  /// The asking account handing over to Atlas, what it means, the work note and how to undo it.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(
        label: state.label,
        title: state.title,
        tail: OnboardingScreen.allowTail,
        lede: state.lede
      ) {
        HStack(spacing: Spacing.mPlus) {
          Disc(content: .letter(state.initial), tone: .avatar, size: .large)
          Image(systemName: "arrow.right")
            .foregroundStyle(Color.textTertiary)
          AppIcon(size: OnboardingLayout.allowIcon)
        }
        .accessibilityHidden(true)
      }
    } bottom: {
      WorkNoteLine(note: note)
        .padding(.top, OnboardingLayout.Rhythm.content)
        .pagePiece(.status)
      Note(text: "Atlas can be removed from both accounts at any time.")
        .padding(.top, OnboardingLayout.Rhythm.status)
        .pagePiece(.status)
    }
  }
}

#if DEBUG
  #Preview("Helper allow") {
    AllowScreen(state: PreviewStates.onboardingAllowScreen, note: .none)
      .onboardingSample()
  }
#endif
