import SwiftUI

/// The route's page under the bar. A new page, an account's included, crossfades in place: it fades in on
/// ``Motion/swapIn`` over the old one fading out on ``Motion/swapOut``, nothing moving, and the new page's rows land
/// where they stand at once rather than travelling there. Under Reduce Motion the page changes at once.
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
        .transition(Self.transition(reduceMotion: reduceMotion))
    }
    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
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

  /// How a page comes and goes: a crossfade in place, its own short animations carried with it so nothing else on the
  /// page animates; at once under Reduce Motion.
  ///
  /// - Parameter reduceMotion: Whether Reduce Motion is on.
  /// - Returns: The transition.
  static func transition(reduceMotion: Bool) -> AnyTransition {
    guard !reduceMotion else { return .identity }
    return .asymmetric(
      insertion: AnyTransition.opacity
        .animation(Motion.swapIn),
      removal: AnyTransition.opacity
        .animation(Motion.swapOut)
    )
  }
}
