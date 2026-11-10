import SwiftUI

/// An account's level as three segments, Look Only, Ask Before Sending and Trusted, the chosen one on the raised
/// thumb that slides to the next as the person picks it.
struct AccountLevelControl: View {
  /// The account's name, which VoiceOver reads with the control.
  let account: String
  /// The level now.
  let level: AccountLevelChoice
  /// Receives the level the person picks.
  let pick: @MainActor (AccountLevelChoice) -> Void

  /// The three segments.
  var body: some View {
    Tabs(state: Self.state(level, account: account), style: .segment) { picked in
      if picked != level { pick(picked) }
    }
    .fixedSize()
  }

  /// The segments, with `level` selected.
  ///
  /// - Parameters:
  ///   - level: The level now.
  ///   - account: The account's name.
  /// - Returns: The tabs' state.
  static func state(_ level: AccountLevelChoice, account: String) -> CapsuleTabsState<AccountLevelChoice> {
    CapsuleTabsState(
      tabs: AccountLevelChoice.allCases.map { CapsuleTab(id: $0, title: $0.title) },
      selected: level,
      label: "What agents may do in \(account)"
    )
  }
}

#if DEBUG
  #Preview("Levels") {
    AskSamples(.levels)
  }
#endif
