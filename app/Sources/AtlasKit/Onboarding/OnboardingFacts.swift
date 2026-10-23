import Foundation

/// What `atlas onboarding status` reports: everything the onboarding window
/// needs to choose its step, read fresh each time.
package struct OnboardingFacts: Equatable, Sendable {
  /// Which side of setup this account is on.
  package var role: Role = .controller
  /// This account's full name, else its short one.
  package var myName = ""
  /// Something wrong with the app itself (moved, or not signed).
  package var appProblem: String?
  /// Whether a helper has been chosen.
  package var configured = false
  /// The accounts that could be the helper.
  package var candidates: [Candidate] = []
  /// The helper chosen before was prepared for this account, so it is the one account offered: no new one.
  package var onlyChoice = false
  /// The helper, as this account sees it.
  package var helper = Helper()
  /// Whether setup has finished.
  package var finished = false
  /// Open at Login was decided once already, so the person's choice stands.
  package var loginDecided = false
  /// In the helper: the account that asked, and this account's own state.
  package var asker = Asker()

  /// Atlas.app is not in Applications, where the helper can run it.
  static let notInApplications =
    "Atlas.app must be in Applications, so the helper account can run it. Move it there, then open it again."
  /// Atlas.app is not signed by its developer.
  static let notSigned = "This copy of Atlas.app is not signed by its developer. Install it again."

  /// Creates facts with nothing known.
  package init() {}

  /// The facts in a status reply; nil when it is not one.
  ///
  /// - Parameter json: The reply as `atlas onboarding status` printed it.
  package init?(json: Data) {
    let decoder = JSONDecoder()
    guard let raw = try? decoder.decode(Reply.self, from: json) else { return nil }
    role = raw.role ?? .controller
    myName = raw.me?.realName ?? raw.me?.name ?? ""
    if raw.app?.installed == false {
      appProblem = Self.notInApplications
    } else if raw.app?.signed == false {
      appProblem = Self.notSigned
    }
    configured = raw.configured ?? false
    candidates = raw.candidates ?? []
    onlyChoice = raw.onlyChoice ?? false
    finished = raw.finished ?? false
    loginDecided = raw.menubarLogin ?? false
    guard let side = raw.helper else { return }
    switch role {
    case .controller:
      helper.exists = side.exists ?? false
      helper.name = side.name ?? ""
      helper.realName = side.realName
      helper.administrator = side.administrator
      helper.leftoverSession = side.leftoverSession ?? false
      helper.paired = side.paired ?? false
      helper.forgettable = side.forgettable ?? false
      helper.signedIn = side.signedIn ?? false
      helper.answering = side.answering ?? false
      helper.onConsole = side.onConsole ?? false
      helper.hidden = side.hidden ?? false
      helper.screenCapture = side.permissions?.screenCapture ?? false
      helper.accessibility = side.permissions?.accessibility ?? false
    case .helper:
      asker.uid = side.controller?.uid ?? 0
      asker.name = side.controller?.name ?? ""
      asker.realName = side.controller?.realName
      asker.installed = side.installed ?? false
      asker.running = side.running ?? false
      asker.screenCapture = side.permissions?.screenCapture ?? false
      asker.accessibility = side.permissions?.accessibility ?? false
      asker.permissionsKnown = side.permissions != nil
    }
  }
}
