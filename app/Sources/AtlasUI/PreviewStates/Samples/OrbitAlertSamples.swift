#if DEBUG
  import SwiftUI

  /// The alert's samples: a plain alert, its action first in blue, and a destructive one, Cancel first in blue.
  struct OrbitAlertSamples: View {
    /// Recover the helper: a plain alert.
    static let recover = OrbitAlertState(
      title: "Recover the helper?",
      message: "An action may not have finished. Atlas checks the helper and starts its mouse and keyboard again.",
      buttons: [
        OrbitAlertButton(id: "recover", title: "Recover", role: .action),
        OrbitAlertButton(id: "later", title: "Not Now", role: .cancel),
      ]
    )

    /// Log out the helper: a destructive alert.
    ///
    /// - Parameter isBusy: Whether its action runs now.
    /// - Returns: The alert.
    static func logOut(isBusy: Bool = false) -> OrbitAlertState<String> {
      OrbitAlertState(
        title: "Log out the helper?",
        message: "Its apps quit. You log in to it again to use it.",
        buttons: [
          OrbitAlertButton(id: "cancel", title: "Cancel", role: .safeCancel),
          OrbitAlertButton(id: "logOut", title: "Log Out", role: .destructive),
        ],
        isBusy: isBusy
      )
    }

    var body: some View {
      HStack(alignment: .top, spacing: Spacing.section) {
        OrbitAlert(state: Self.recover) { _ in }
        OrbitAlert(state: Self.logOut()) { _ in }
      }
      .padding(Spacing.page)
      .background(Color.scrim)
      .sampleSurface()
    }
  }
#endif
