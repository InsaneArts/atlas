import SwiftUI

/// What an account's menu offers beyond its level: for a site, Sign In…, Forget This Site… and Reset the Helper's
/// Browser…, each run with agents paused; then Remove from List….
struct AccountActions: View {
  /// The account.
  let row: SettingsAccountRow
  /// Receives what the person picks.
  let send: @MainActor (SettingsEvent) -> Void

  /// The items.
  var body: some View {
    if row.isSite {
      Button("Sign In…") { send(.askSignIn(row.title)) }
      Button("Forget This Site…") { send(.forgetAccount(row.id)) }
      Button("Reset the Helper's Browser…") { send(.resetBrowser(row.id)) }
      Divider()
    }
    Button("Remove from List…", role: .destructive) { send(.askRemoveAccount(row.id)) }
  }
}
