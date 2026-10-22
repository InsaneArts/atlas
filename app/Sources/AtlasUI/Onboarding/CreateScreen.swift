import SwiftUI

/// The form for a new helper account. Its four fields edit the window's `OnboardingAccountForm` in place.
struct CreateScreen: View {
  /// What the screen shows around the form.
  let state: OnboardingCreateScreenState
  /// The account's names and password.
  @Binding var form: OnboardingAccountForm

  /// The form, then the line under it.
  var body: some View {
    ScreenColumn {
      GroupLabel(text: "New account")
      Headline(title: OnboardingScreen.createTitle, tail: state.tail)
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
      .opacity(state.isFormEnabled ? 1 : Opacity.busy)
      .padding(.top, Spacing.s)
    } bottom: {
      WorkNoteLine(note: state.note)
      Note(
        text:
          "You log in to the helper with this password, now and after each restart. Atlas hands it to macOS and never keeps it."
      )
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
