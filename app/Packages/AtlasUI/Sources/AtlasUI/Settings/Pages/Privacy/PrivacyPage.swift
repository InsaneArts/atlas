import SwiftUI

/// The Privacy page: "Your Files", the home folder private from the helper with Undo… or open to it with Make Private,
/// and what this setting doesn't cover; then "Sign-Ins", whether agents may copy sign-ins in, and what that lets them
/// do. Private and open cross-fade in place, the row keeping its height and the button its width; while a change runs,
/// a spinner takes the button's title's place. A change that failed fades its line in under "Your Files" on
/// ``Motion/fade`` while what it moves goes on ``Motion/state``; under Reduce Motion that layout changes at once.
struct PrivacyPage: View {
  /// What the page shows; nil until Atlas has looked.
  let state: SettingsFilesState?
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The two sections, or a line while Atlas looks.
  var body: some View {
    if let state {
      VStack(alignment: .leading, spacing: SettingsLayout.sectionGap) {
        if let home = state.home {
          GroupedSection([home], title: "Your Files", notes: filesNotes(state)) { home in
            homeRow(home, canRun: home.canRun)
          }
        }
        if let isOn = state.signInCopies {
          GroupedSection(
            [PrivacySignIns(isOn: isOn)],
            title: "Sign-Ins",
            notes: [
              .text(
                "When on, an agent could sign the helper in to your accounts without you, by copying a browser's cookies."
              )
            ]
          ) { signIns in
            signInsRow(signIns.isOn, canChange: state.canChangeSignIns)
          }
        }
      }
      .animation(Self.layoutAnimation(reduceMotion: reduceMotion), value: Self.layout(state))
    } else {
      StatusLine(kind: .working, text: "Checking…", size: .small)
        .padding(.horizontal, ListRow.Size.compact.inset)
    }
  }

  /// How the page's layout moves as rows and notes come and go: on ``Motion/state``, or at once under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The animation, or nil for at once.
  static func layoutAnimation(reduceMotion: Bool) -> Animation? {
    Motion.animation(Motion.state, reduceMotion: reduceMotion)
  }

  /// What changes the page's layout: which rows and which line show.
  ///
  /// - Parameter state: The page.
  /// - Returns: Whether the home row, the sign-ins row and a failure's line show.
  static func layout(_ state: SettingsFilesState) -> [Bool] {
    [state.home != nil, state.signInCopies != nil, state.status != nil]
  }

  /// The lines under "Your Files": what the last change said, then what the setting doesn't cover.
  ///
  /// - Parameter state: The page.
  /// - Returns: The lines.
  private func filesNotes(_ state: SettingsFilesState) -> [GroupedSectionNote] {
    [state.status?.sectionNote]
      .compactMap { $0 }
      + [.text("Only your home folder. /Users/Shared and external drives keep their own permissions.")]
  }

  /// The home folder's row: Undo… while private, Make Private in the accent while open.
  ///
  /// - Parameters:
  ///   - home: The row.
  ///   - canRun: Whether its button takes presses now.
  /// - Returns: The row.
  private func homeRow(_ home: SettingsFilesRow, canRun: Bool) -> some View {
    PrivacyFilesRow(title: home.title, detail: home.detail, swap: home.isPrivate) {
      Button {
        send(home.isPrivate ? .askOpenHome : .makeHomePrivate)
      } label: {
        ZStack {
          Text(home.heldButton)
            .hidden()
          WorkingLabel(title: home.button, isWorking: home.isWorking, workingTitle: "Changing…")
            .swapsText(on: home.button)
        }
      }
      .buttonStyle(.capsule(home.isPrivate ? .secondary : .accent, size: .row, isWorking: home.isWorking))
      .fixedSize()
      .disabled(!canRun)
      .layoutFrame("files home button")
    }
    .layoutFrame("files home row")
  }

  /// Whether agents may copy sign-ins into the helper, with its switch.
  ///
  /// - Parameters:
  ///   - isOn: Whether they may.
  ///   - canChange: Whether the switch takes changes now.
  /// - Returns: The row.
  private func signInsRow(_ isOn: Bool, canChange: Bool) -> some View {
    let title = "Copy sign-ins into the helper"
    return PrivacyFilesRow(
      title: title,
      detail: "Off keeps browser profiles, keychains, .env and .npmrc out",
      swap: isOn
    ) {
      ListRowTrailing(trailing: .toggle(isOn: isOn), label: title, size: .compact) { send(.setSignInCopies(!isOn)) }
    }
    .disabled(!canChange)
  }
}

#if DEBUG
  #Preview("Privacy, private") {
    SettingsPageSample { PrivacyPage(state: PreviewStates.settingsFilesPrivate) { _ in } }
  }

  #Preview("Privacy, open") {
    SettingsPageSample { PrivacyPage(state: PreviewStates.settingsFilesOpen) { _ in } }
  }

  #Preview("Privacy, failed, dark") {
    SettingsPageSample { PrivacyPage(state: PreviewStates.settingsFilesFailed) { _ in } }
      .environment(\.colorScheme, .dark)
  }
#endif
