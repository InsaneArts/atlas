import SwiftUI

/// The form for a new helper account, in one group (v3 D6). Its four fields edit the window's `OnboardingAccountForm`
/// in place; a field the status is about washes orange and takes the focus (v3 X12). While the account is made the
/// form dims. Create Account is the footer's.
struct CreateScreen: View {
  /// What the screen shows around the form.
  let state: OnboardingCreateScreenState
  /// The account's names and password.
  @Binding var form: OnboardingAccountForm
  /// The field the person types in, if any, which a flag never takes the focus from.
  @State private var typingIn: AccountFormField?

  /// The form, the status under it, then why macOS will ask for an administrator's approval. The three scroll as one
  /// when the window is too short for them all, so the card goes under the fade, never the form or its status (v3
  /// `03-create-bad-name`).
  var body: some View {
    ScrollColumn(width: OnboardingLayout.column) {
      OnboardingHeader(label: "New account", title: OnboardingScreen.createTitle, tail: state.tail)
      GroupedList(AccountFormField.allCases) { field in
        FieldRow(
          label: field.label,
          text: $form[dynamicMember: field.value],
          secure: field.isSecure,
          placeholder: field.placeholder,
          isFlagged: field.isFlagged(state.flagged),
          isOtherFieldFocused: typingIn.map { $0 != field } ?? false
        ) { focused in
          typingIn = Self.typingIn(after: typingIn, field: field, focused: focused)
        }
      }
      .disabled(!state.isFormEnabled)
      .opacity(state.isFormEnabled ? 1 : Opacity.muted)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      .pagePiece(.content)
      if let status = StatusLine(note: state.note) {
        status
          .padding(.top, OnboardingLayout.Rhythm.content)
          .pagePiece(.status)
      }
      ApprovalNote()
        .padding(.top, OnboardingLayout.Rhythm.content)
        .pagePiece(.status)
    }
  }

  /// Where the person types once a field takes or loses the focus.
  ///
  /// - Parameters:
  ///   - current: The field typed in until now, if any.
  ///   - field: The field whose focus changed.
  ///   - focused: Whether it took the focus.
  /// - Returns: The field typed in now, if any.
  private static func typingIn(
    after current: AccountFormField?,
    field: AccountFormField,
    focused: Bool
  ) -> AccountFormField? {
    if focused { return field }
    return current == field ? nil : current
  }
}

#if DEBUG
  #Preview("Create") {
    @Previewable @State var form = PreviewStates.onboardingForm
    CreateScreen(state: PreviewStates.onboardingCreateScreen, form: $form)
      .onboardingSample()
  }

  #Preview("Bad name") {
    @Previewable @State var form = PreviewStates.onboardingFormBadName
    if case .screen(.create(let state)) = PreviewStates.onboardingCreateBadName.content {
      CreateScreen(state: state, form: $form)
        .onboardingSample()
    }
  }
#endif
