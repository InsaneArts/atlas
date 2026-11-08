import AtlasService
import Foundation

/// What Atlas shows outside its windows of the questions it asks the person: a notification for each new one the
/// person isn't looking at, posted once macOS lets Atlas notify them and taken away once it is answered or ends,
/// whatever Notify me says; and, when the person said no
/// to notifications, the menu bar's badge while one is up (``AskStore/notificationsRefused``). macOS is asked whether
/// Atlas may notify them once, the first time an account joins the list.
final class AskSignals {
  /// The notifications.
  private let notifications: AskNotifications
  /// Whether macOS was asked already, kept across launches.
  private let preferences: SettingsPreferences
  /// The store the menu bar's badge follows.
  private let store: AskStore
  /// The id of the question the person sees now, in the open panel or over the viewer's Screen; nil for none.
  private let inView: @MainActor () -> String?
  /// Whether macOS lets Atlas notify the person: nil until it is known, and while it is being asked.
  private(set) var consent: Bool?
  /// The questions delivered with consent and not yet taken away.
  private(set) var posted: Set<String> = []
  /// Whether macOS is being asked now, so it is asked once.
  private var asking = false
  /// Whether the notifications an earlier launch left were matched with the questions up, which the first reading
  /// does.
  private var reconciled = false

  /// Signals that have posted nothing yet.
  ///
  /// - Parameters:
  ///   - notifications: The notifications.
  ///   - preferences: Where asking macOS once is kept.
  ///   - store: The store the menu bar's badge follows.
  ///   - inView: The id of the question the person sees now; nil for none.
  init(
    notifications: AskNotifications,
    preferences: SettingsPreferences,
    store: AskStore,
    inView: @escaping @MainActor () -> String?
  ) {
    self.notifications = notifications
    self.preferences = preferences
    self.store = store
    self.inView = inView
  }

  /// Reads whether the person let Atlas notify them, as macOS last said, for the menu bar's badge; once it says yes,
  /// every question still up and not seen posts. Nothing is read while macOS is being asked.
  func readConsent() async {
    guard !asking else { return }
    resolve(await notifications.consent())
  }

  /// Takes what macOS said: the badge follows a no, and a yes posts every question still up and not seen.
  ///
  /// - Parameter answer: Whether Atlas may notify the person; nil while undecided.
  func resolve(_ answer: Bool?) {
    consent = answer
    store.notificationsRefused = answer == false
    if answer == true, let list = store.list { post(list) }
  }

  /// The questions changed: each new one posts while the panel is closed, and each gone is taken away. The first time,
  /// after the first reading, any notification an earlier launch left whose question is no longer up goes too.
  ///
  /// - Parameter list: The questions up now.
  func asksChanged(_ list: AskList) {
    let ids = Set(list.asks.map(\.id))
    if !reconciled {
      reconciled = true
      // Unstructured, as Notification Center answers later; questions answered meanwhile are withdrawn as they go.
      Task { await withdrawLeftovers() }
    }
    let gone = posted.subtracting(ids)
    if !gone.isEmpty { notifications.withdraw(gone.sorted()) }
    posted.formIntersection(ids)
    guard consent == true else {
      // macOS may have been answered elsewhere since, as through Notify me: read it again for what waits.
      if !asking, list.asks.contains(where: { !posted.contains($0.id) }) { Task { await readConsent() } }
      return
    }
    post(list)
  }

  /// The person may have looked away from a question they saw, as the panel closed or the main window left the Screen,
  /// was minimised, covered or closed: it posts now, by the same rules, if it is still up and not seen.
  func lookedAway() {
    if let list = store.list { post(list) }
  }

  /// Posts each question up that isn't posted yet and that the person isn't looking at; only with consent, so a
  /// question counts as posted once macOS could deliver it.
  ///
  /// - Parameter list: The questions up now.
  private func post(_ list: AskList) {
    guard consent == true else { return }
    let seen = inView()
    for ask in list.asks where !posted.contains(ask.id) && ask.id != seen {
      notifications.post(AskNotifications.note(ask))
      posted.insert(ask.id)
    }
  }

  /// Takes away every notification Notification Center still shows whose question is not up now, and keeps track of
  /// the rest as posted.
  func withdrawLeftovers() async {
    let delivered = await notifications.delivered()
    let open = Set(store.list?.asks.map(\.id) ?? Array(posted))
    let leftovers = delivered.filter { !open.contains($0) }
    if !leftovers.isEmpty { notifications.withdraw(leftovers) }
    // The ones kept are this launch's to take away once their question is answered or ends.
    posted.formUnion(delivered.filter { open.contains($0) })
  }

  /// A status was read: the first time it lists an account, macOS is asked, once, whether Atlas may notify the
  /// person of its questions.
  ///
  /// - Parameter status: The service's answer.
  func statusRead(_ status: ViewerStatus) async {
    guard let accounts = status.overview.accounts, !accounts.isEmpty else { return }
    guard !asking, !preferences.loadAskConsentAsked() else { return }
    asking = true
    consent = nil
    preferences.saveAskConsentAsked()
    let allowed = await notifications.requestConsent()
    asking = false
    resolve(allowed)
  }
}
