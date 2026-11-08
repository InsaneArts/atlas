import AtlasService
import Foundation

/// What Atlas shows outside its windows of the questions it asks the person: a notification for each new one while
/// the panel is closed, taken away once it is answered or ends, whatever Notify me says; and, when the person said no
/// to notifications, the menu bar's badge while one is up (``AskStore/notificationsRefused``). macOS is asked whether
/// Atlas may notify them once, the first time an account joins the list.
final class AskSignals {
  /// The notifications.
  private let notifications: AskNotifications
  /// Whether macOS was asked already, kept across launches.
  private let preferences: SettingsPreferences
  /// The store the menu bar's badge follows.
  private let store: AskStore
  /// Whether the panel is open now, which shows the question itself.
  private let panelOpen: @MainActor () -> Bool
  /// The questions posted and not yet taken away.
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
  ///   - panelOpen: Whether the panel is open now.
  init(
    notifications: AskNotifications,
    preferences: SettingsPreferences,
    store: AskStore,
    panelOpen: @escaping @MainActor () -> Bool
  ) {
    self.notifications = notifications
    self.preferences = preferences
    self.store = store
    self.panelOpen = panelOpen
  }

  /// Reads whether the person let Atlas notify them, as macOS last said, for the menu bar's badge.
  func readConsent() async {
    store.notificationsRefused = await notifications.consent() == false
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
    guard !panelOpen() else { return }
    for ask in list.asks where !posted.contains(ask.id) {
      notifications.post(AskNotifications.note(ask))
      posted.insert(ask.id)
    }
  }

  /// Takes away every notification Notification Center still shows whose question is not up now.
  func withdrawLeftovers() async {
    let delivered = await notifications.delivered()
    let open = Set(store.list?.asks.map(\.id) ?? Array(posted))
    let leftovers = delivered.filter { !open.contains($0) }
    if !leftovers.isEmpty { notifications.withdraw(leftovers) }
  }

  /// A status was read: the first time it lists an account, macOS is asked, once, whether Atlas may notify the
  /// person of its questions.
  ///
  /// - Parameter status: The service's answer.
  func statusRead(_ status: ViewerStatus) async {
    guard let accounts = status.overview.accounts, !accounts.isEmpty else { return }
    guard !asking, !preferences.loadAskConsentAsked() else { return }
    asking = true
    preferences.saveAskConsentAsked()
    let allowed = await notifications.requestConsent()
    store.notificationsRefused = !allowed
    asking = false
  }
}
