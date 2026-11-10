import SwiftUI

/// The Settings window: the sidebar of pages on the window's recessed ground, and beside it the content card with back
/// and forward, the route's title and its page; an alert or Sign In to a Site…'s sheet over the whole window when one
/// is asked. It draws its state and sends what the person does; the General and Activity pages' values change through
/// `edit`. What a command says, and what an alert asks, VoiceOver hears once, as it changes.
public struct SettingsView: View {
  /// What the window shows.
  private let state: SettingsState
  /// Receives the General and Activity pages as the person changed them; the controller writes each value where it
  /// belongs.
  private let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives what the person does.
  private let send: @MainActor (SettingsEvent) -> Void

  /// Receives explicit results and alerts for VoiceOver, independently of routine helper status.
  private let announce: @MainActor (String) -> Void

  /// Creates the window's view.
  ///
  /// - Parameters:
  ///   - state: What the window shows.
  ///   - edit: Receives the General and Activity pages as the person changed them.
  ///   - announce: Posts explicit results and alerts to VoiceOver.
  ///   - send: Receives what the person does.
  public init(
    state: SettingsState,
    edit: @escaping @MainActor (SettingsGeneralState) -> Void,
    announce: @escaping @MainActor (String) -> Void = {
      AccessibilityNotification.Announcement($0)
        .post()
    },
    send: @escaping @MainActor (SettingsEvent) -> Void
  ) {
    self.state = state
    self.edit = edit
    self.send = send
    self.announce = announce
  }

  /// The sidebar beside the card, and the alert or the sheet over both when one is asked.
  public var body: some View {
    ZStack {
      HStack(spacing: .zero) {
        SettingsSidebar(state: state.sidebar, page: state.route.page, send: send)
        SettingsCard(state: state, edit: edit, send: send)
          .padding(.vertical, WindowChrome.cardInset)
          .padding(.trailing, WindowChrome.cardInset)
      }
      // Under an alert or the sheet, nothing is reached by Tab or VoiceOver until it is answered.
      .disabled(state.alert != nil || state.signIn != nil)
      .accessibilityHidden(state.alert != nil || state.signIn != nil)
      SignInSheetLayer(state: state.signIn, send: send)
      AlertLayer(state: state.alert) { send($0 == .confirm ? .alertConfirm : .alertCancel) }
    }
    .background(Color.groundRecessed)
    .foregroundStyle(Color.textPrimary)
    // The sidebar and the bar sit in the title bar, beside the window buttons.
    .ignoresSafeArea(.container, edges: .top)
    // Each only when its words change, so a poll that reads the same says nothing.
    .onChange(of: state.alert?.shownTitle) { _, title in if let title { announce(title) } }
    .onChange(of: state.alert?.refusal) { _, refusal in if let refusal { announce(refusal) } }
    .onChange(of: state.signIn?.refusal) { _, refusal in if let refusal { announce(refusal) } }
    .onChange(of: state.helper.waiting) { _, waiting in if let waiting { announce(waiting) } }
    .onChange(of: state.helper.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.general.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.accounts.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.account?.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.privacy?.status) { _, status in if let status { announce(status.text) } }
    .onChange(of: state.agents.notice) { _, notice in if let notice { announce(notice.text) } }
    .onChange(of: state.about.status) { _, status in if let status { announce(status.text) } }
  }
}

#if DEBUG
  #Preview("Helper") {
    SettingsView(state: PreviewStates.settingsHelper, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.width, height: SettingsLayout.height)
  }

  #Preview("General, dark") {
    SettingsView(state: PreviewStates.settingsGeneral, edit: { _ in }, send: { _ in })
      .frame(width: SettingsLayout.width, height: SettingsLayout.height)
      .environment(\.colorScheme, .dark)
  }

  #Preview("Interactive") {
    @Previewable @State var state = PreviewStates.settingsHelper
    SettingsView(state: state, edit: { state.general = $0 }) { event in
      state = PreviewStates.settings(state, applying: event)
    }
    .frame(width: SettingsLayout.width, height: SettingsLayout.height)
  }
#endif
