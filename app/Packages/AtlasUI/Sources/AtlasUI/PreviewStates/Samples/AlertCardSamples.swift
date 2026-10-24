#if DEBUG
  import SwiftUI

  /// The alert's samples: a plain alert with short labels side by side, a destructive one, and a stacked one, each
  /// asked, plus Recover running, refused and done.
  struct AlertCardSamples: View {
    /// Recover's running word.
    static let recovering = "Recovering…"
    /// Recover's done title.
    static let recovered = "Recovered"
    /// Why a Recover was refused.
    static let refusal = "The helper is in use. Try Recover again once it's free."

    /// Recover the helper: a plain alert.
    static let recover = AlertCardState(
      title: "Recover the helper?",
      message: "An action may not have finished. Atlas checks the helper and starts its mouse and keyboard again.",
      buttons: [
        AlertCardButton(id: "recover", title: "Recover", role: .action),
        AlertCardButton(id: "later", title: "Not Now", role: .cancel),
      ]
    )

    /// Log out the helper: a destructive alert.
    static let logOut = AlertCardState(
      title: "Log out the helper?",
      message: "Its apps quit, and agents can't use it until you log in to it again.",
      buttons: [
        AlertCardButton(id: "cancel", title: "Cancel", role: .safeCancel),
        AlertCardButton(id: "logOut", title: "Log Out", role: .destructive),
      ]
    )

    /// An app held the log-out up: a label too long to share a row stacks the buttons.
    static let logOutAnyway = AlertCardState(
      title: "The helper didn't log out",
      message: "An app in the helper is holding it up. Log Out Anyway quits it, and anything unsaved there is lost.",
      buttons: [
        AlertCardButton(id: "cancel", title: "Cancel", role: .safeCancel),
        AlertCardButton(id: "logOut", title: "Log Out Anyway", role: .destructive),
      ]
    )

    /// Recover in each of its phases after it is asked.
    static let phases: [AlertCardState<String>] = [
      AlertCardPhase.running(label: recovering), .refused(message: refusal), .done(title: recovered),
    ]
    .map { phase in
      var state = recover
      state.phase = phase
      return state
    }

    var body: some View {
      VStack(alignment: .leading, spacing: Spacing.xxl) {
        HStack(alignment: .top, spacing: Spacing.xxl) {
          AlertCard(state: Self.recover) { _ in }
          AlertCard(state: Self.logOut) { _ in }
          AlertCard(state: Self.logOutAnyway) { _ in }
        }
        HStack(alignment: .top, spacing: Spacing.xxl) {
          ForEach(Self.phases, id: \.phase) { state in
            AlertCard(state: state) { _ in }
          }
        }
      }
      .padding(Spacing.xxxl)
      .background(Color.scrim)
      .sampleSurface()
    }
  }
#endif
