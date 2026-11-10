import SwiftUI

/// Sign In to a Site…: its title, the note on what Atlas does meanwhile, a capsule field for the site, why Atlas
/// couldn't start when it couldn't, and Cancel and Continue, on a card of the alerts' family. The field takes the
/// keyboard as the sheet appears; Return continues and Escape cancels. It draws its state and sends what the person
/// does: each edit, Continue and Cancel.
struct SignInSheet: View {
  /// What the sheet shows.
  let state: SignInSheetState
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void
  @FocusState private var focused: Bool

  /// What Atlas does while the person signs in.
  static let note =
    "Atlas pauses agents and hides the helper's screen from them while you sign in. "
    + "Paste from your password manager works."

  /// The card.
  var body: some View {
    VStack(alignment: .leading, spacing: SignInSheetLayout.gap) {
      VStack(alignment: .leading, spacing: SignInSheetLayout.titleGap) {
        Text("Sign in to a site")
          .textStyle(.heading)
          .foregroundStyle(Color.textPrimary)
        Text(Self.note)
          .textStyle(.callout)
          .foregroundStyle(Color.textSecondary)
          .fixedSize(horizontal: false, vertical: true)
      }
      field
      if let refusal = state.refusal {
        Text(refusal)
          .textStyle(.callout)
          .foregroundStyle(Color.textAttention)
          .fixedSize(horizontal: false, vertical: true)
          .transition(.opacity)
      }
      HStack(spacing: SignInSheetLayout.buttonGap) {
        Spacer(minLength: .zero)
        Button("Cancel") { send(.cancelSignIn) }
          .buttonStyle(.capsule(.secondary, size: .compact))
          .keyboardShortcut(.cancelAction)
        Button("Continue") { send(.continueSignIn) }
          .buttonStyle(.capsule(.accent, size: .compact, isWorking: state.isWorking))
          .keyboardShortcut(.defaultAction)
          .disabled(!state.canContinue)
      }
    }
    .animation(Motion.fade, value: state.refusal)
    .padding(SignInSheetLayout.inset)
    .frame(width: SignInSheetLayout.width)
    .surface(.modal, in: shape)
    .overlay(
      shape
        .strokeBorder(Color.strokeStrong, lineWidth: Stroke.hairline)
        .allowsHitTesting(false)
    )
    .background { CutoutShadow(shape: shape, level: .modal) }
    .onAppear { focused = true }
    .accessibilityElement(children: .contain)
    .accessibilityAddTraits(.isModal)
  }

  /// The site's field: a quiet capsule, ringed in the accent while it takes the keyboard.
  private var field: some View {
    TextField(
      text: Binding(get: { state.site }, set: { send(.editSignInSite($0)) }),
      prompt: Text(verbatim: "github.com")
        .foregroundStyle(Color.textTertiary)
    ) {
      Text("Site")
    }
    .textFieldStyle(.plain)
    .font(Typography.font(.body))
    .autocorrectionDisabled()
    .focused($focused)
    .disabled(state.isWorking)
    .padding(.horizontal, SignInSheetLayout.fieldInset)
    .frame(height: ControlHeight.s)
    .background(
      Capsule()
        .fill(Color.fillControl)
    )
    .overlay {
      CapsuleEdge()
        .strokeBorder(Color.strokeFocus, lineWidth: Stroke.regular)
        .background(
          CapsuleEdge()
            .stroke(Color.accentRing, lineWidth: SignInSheetLayout.haloWidth)
        )
        .opacity(focused ? 1 : 0)
        .allowsHitTesting(false)
    }
    .animation(Motion.fade, value: focused)
  }

  /// The card's outline.
  private var shape: RoundedRectangle {
    RoundedRectangle(cornerRadius: Radius.extraLarge, style: .continuous)
  }
}
