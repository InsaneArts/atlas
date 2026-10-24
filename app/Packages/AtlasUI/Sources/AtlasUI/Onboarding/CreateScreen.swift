import SwiftUI

/// The form for a new helper account. Its four fields edit the window's `OnboardingAccountForm` in place. Create
/// Account is the footer's.
struct CreateScreen: View {
  /// What the screen shows around the form.
  let state: OnboardingCreateScreenState
  /// The account's names and password.
  @Binding var form: OnboardingAccountForm

  /// The form, the line under it, then why macOS will ask for an administrator's approval.
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(label: "New account", title: OnboardingScreen.createTitle, tail: state.tail)
      CardGroup {
        FieldRow(label: "Full name", text: $form.fullName)
        SeparatorLine()
        FieldRow(label: "Account name", text: $form.accountName, isFlagged: state.flagged == .accountName)
        SeparatorLine()
        FieldRow(label: "Password", text: $form.password, secure: true)
        SeparatorLine()
        FieldRow(label: "Verify", text: $form.verify, secure: true, isFlagged: state.flagged == .verify)
      }
      .disabled(!state.isFormEnabled)
      .opacity(state.isFormEnabled ? 1 : Opacity.muted)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
    } bottom: {
      WorkNoteLine(note: state.note)
        .padding(.top, OnboardingLayout.Rhythm.content)
        .pagePiece(.status)
      ApprovalNote()
        .padding(.top, OnboardingLayout.Rhythm.status)
        .pagePiece(.status)
    }
  }
}

#if DEBUG
  #Preview("Create") {
    @Previewable @State var form = PreviewStates.onboardingForm
    CreateScreen(state: PreviewStates.onboardingCreateScreen, form: $form)
      .onboardingSample()
  }
#endif
