import Foundation

/// Details' facts that the helper's status does not carry: the agents connected to Atlas and when the helper logged
/// in. They are read when Details opens and by Check Again, never with every reading.
extension ViewerController {
  /// Reads the agents connected to Atlas and the helper's login time. A reading that fails keeps what was known.
  func readDetails() async {
    readLoginTime()
    if let read = await dependencies.connectedAgents() { details.agents = read }
  }

  /// Reads when the helper's account logged in, once its account is known.
  func readLoginTime() {
    guard let account = reading.status.overview.account else { return }
    details.loggedInAt = dependencies.loginTime(account)
  }
}
