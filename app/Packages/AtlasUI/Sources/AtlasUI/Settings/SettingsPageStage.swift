import SwiftUI

/// The route's page under the bar. A new page comes in on ``Motion/swapIn``, rising ``SettingsLayout/pageRise`` as it
/// fades in, over the old one fading out on ``Motion/swapOut``; an account's page slides in
/// ``SettingsLayout/pushShift`` from the trailing edge as it fades in, and slides back out the same way. Under Reduce
/// Motion each only crossfades.
struct SettingsPageStage: View {
  /// What the window shows.
  let state: SettingsState
  /// Receives the General and Activity pages as the person changed them.
  let edit: @MainActor (SettingsGeneralState) -> Void
  /// Receives what the person does.
  let send: @MainActor (SettingsEvent) -> Void
  @Environment(\.motionReduced) private var reduceMotion

  /// The page, swapped by its route.
  var body: some View {
    ZStack(alignment: .top) {
      page
        .id(state.route)
        .transition(Self.transition(for: state.route, reduceMotion: reduceMotion))
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    .animation(Motion.swapIn, value: state.route)
  }

  /// The route's page in its scrolling column.
  @ViewBuilder private var page: some View {
    let general = Binding(get: { state.general }, set: { edit($0) })
    SettingsPageScroll {
      switch state.route {
      case .page(.helper): HelperPage(state: state.helper, send: send)
      case .page(.general): GeneralPage(state: general)
      case .page(.activity): ActivityPage(state: general, send: send)
      case .page(.accounts): AccountsPage(state: state.accounts, send: send)
      case .account:
        // An account Atlas has never listed while this window was open has no page to keep; Accounts says why.
        if let account = state.account {
          AccountPage(state: account, send: send)
        } else {
          AccountsPage(state: state.accounts, send: send)
        }
      case .page(.privacy): PrivacyPage(state: state.privacy, send: send)
      case .page(.integrations): IntegrationsPage(state: state.agents, send: send)
      case .page(.about): AboutPage(state: state.about, send: send)
      }
    }
  }

  /// How a route's page comes and goes: an account's slides from the trailing edge, any other rises; both fade, and
  /// under Reduce Motion only fade.
  ///
  /// - Parameters:
  ///   - route: The route.
  ///   - reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The transition.
  static func transition(for route: SettingsRoute, reduceMotion: Bool) -> AnyTransition {
    let move: AnyTransition =
      switch route {
      case .account: .offset(x: SettingsLayout.pushShift)
      case .page: .offset(y: SettingsLayout.pageRise)
      }
    let insertion = reduceMotion ? AnyTransition.opacity : move.combined(with: .opacity)
    let removal = route.account != nil && !reduceMotion ? move.combined(with: .opacity) : .opacity
    return .asymmetric(
      insertion: insertion.animation(Motion.swapIn),
      removal: removal.animation(Motion.swapOut)
    )
  }
}
