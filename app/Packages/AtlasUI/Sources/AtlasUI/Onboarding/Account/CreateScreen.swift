import SwiftUI

/// The form for a new helper account, in one group. Its four fields edit the window's `OnboardingAccountForm` in
/// place, and the first empty one takes the focus as the form appears; a field the status is about washes orange and
/// takes the focus. While the account is made the form dims. Create Account is the footer's.
struct CreateScreen: View {
  /// What the screen shows around the form.
  let state: OnboardingCreateScreenState
  /// The account's names and password.
  @Binding var form: OnboardingAccountForm
  /// The field the person types in, if any, which a flag never takes the focus from.
  @State private var typingIn: AccountFormField?

  /// The form, the status under it in its kept place, then why macOS will ask for a password.
  var body: some View {
    ScreenColumn {
      OnboardingHeader(label: "New account", title: OnboardingScreen.createTitle, tail: state.tail)
      GroupedList(AccountFormField.allCases) { field in
        FieldRow(
          label: field.label,
          text: $form[dynamicMember: field.value],
          secure: field.isSecure,
          placeholder: field.placeholder,
          isFlagged: field.isFlagged(state.flagged),
          isOtherFieldFocused: typingIn.map { $0 != field } ?? false,
          focusesOnAppear: field == Self.firstEmpty(form)
        ) { focused in
          typingIn = Self.typingIn(after: typingIn, field: field, focused: focused)
        }
      }
      .disabled(!state.isFormEnabled)
      .opacity(state.isFormEnabled ? 1 : Opacity.muted)
      .padding(.top, OnboardingLayout.Rhythm.paragraph)
      StatusSlot(note: state.note)
      ApprovalNote()
        .padding(.top, OnboardingLayout.Rhythm.content)
    }
  }

  /// The form's first empty field, where the person starts typing: the password, once both names are filled in.
  ///
  /// - Parameter form: The form as it reads.
  /// - Returns: The field, or nil when every field is filled in.
  static func firstEmpty(_ form: OnboardingAccountForm) -> AccountFormField? {
    AccountFormField.allCases
      .first { form[keyPath: $0.value].isEmpty }
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
