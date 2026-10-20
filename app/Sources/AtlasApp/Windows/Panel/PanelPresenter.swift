import AtlasKit
import AtlasUI

/// Builds the panel's state from what its controller holds. Pure: every choice the panel shows is made here, from
/// the snapshot and AtlasKit's rules (``Mode``, ``StatusReading``, ``Words`` and the log-out policy).
enum PanelPresenter {
  /// The panel as it should look now.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Everything the panel draws.
  static func state(_ snapshot: PanelSnapshot) -> PanelState {
    let mode = snapshot.status.overview.mode
    return PanelState(
      status: Mode.of(mode).summary,
      reason: reason(snapshot.status.overview),
      stage: stage(snapshot),
      recovery: recovery(snapshot.status.overview),
      facts: facts(snapshot.status),
      isViewerPrimary: mode != .loggedOut,
      secondary: secondary(snapshot),
      logOut: logOut(snapshot),
      // The spinner shows while any action runs, whichever said the words.
      feedback: snapshot.feedback.map { Feedback(text: $0, isWorking: snapshot.working) },
      menu: menu(snapshot)
    )
  }

  /// The reason under the title: hidden when empty, and when the recovery card or the way to log in says it.
  ///
  /// - Parameter overview: The service's overview.
  /// - Returns: The reason, or nil to hide the line.
  private static func reason(_ overview: ViewerStatus.Overview) -> String? {
    let hidden = overview.reason.isEmpty || offersRecovery(overview) || overview.mode == .loggedOut
    return hidden ? nil : overview.reason
  }

  /// Whether the recovery card shows: while recovering, or when Recover fixes what keeps the helper not ready.
  ///
  /// - Parameter overview: The service's overview.
  /// - Returns: Whether the card offers Recover.
  private static func offersRecovery(_ overview: ViewerStatus.Overview) -> Bool {
    overview.mode == .recovering || (overview.mode == .blocked && overview.recoverable)
  }

  /// What fills the space under the header: logged out, the way to log in; else the desktop or why there is none.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The stage.
  private static func stage(_ snapshot: PanelSnapshot) -> PanelStage {
    if snapshot.status.overview.mode == .loggedOut {
      return .loggedOut(text: Words.loggedOut, isEnabled: !snapshot.working)
    }
    return snapshot.picture.map(PanelStage.picture) ?? .placeholder(snapshot.placeholder)
  }

  /// Why the recovery card offers Recover: an unknown outcome, or input Recover starts again.
  ///
  /// - Parameter overview: The service's overview.
  /// - Returns: The reason, a fallback when it is empty, or nil to hide the card.
  private static func recovery(_ overview: ViewerStatus.Overview) -> PanelRecovery? {
    guard offersRecovery(overview) else { return nil }
    if overview.mode == .blocked {
      return .notReady(overview.reason.isEmpty ? PanelWords.inputStopped : overview.reason)
    }
    return .unfinished(overview.reason.isEmpty ? PanelWords.recoverFirst : overview.reason)
  }

  /// The helper, its display and the newest action, with a dash for what is unknown.
  ///
  /// - Parameter status: The service's status.
  /// - Returns: The three facts, in order.
  private static func facts(_ status: ViewerStatus) -> [Fact] {
    let overview = status.overview
    return [
      Fact(name: PanelWords.helperFact, value: overview.title ?? Words.unknown),
      Fact(name: Words.displayFact, value: Words.size(overview.display) ?? Words.unknown),
      Fact(name: PanelWords.lastActionFact, value: StatusReading(status).lastAction),
    ]
  }

  /// The button beside Open Viewer: Release Control, which revokes it, while a viewer holds control; Switch to
  /// Helper… unless the helper is logged out, waiting for a known helper and an idle panel.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The button, or nil for none.
  private static func secondary(_ snapshot: PanelSnapshot) -> PanelSecondaryAction? {
    switch snapshot.status.overview.mode {
    case .controlling: .revokeControl
    case .loggedOut: nil
    case .observing, .agent, .recovering, .blocked, .unavailable:
      .switchToHelper(isEnabled: snapshot.helper != nil && !snapshot.working)
    }
  }

  /// The confirmation or Log Out Anyway, each waiting while a command runs or the mode forbids it.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: Where logging out stands, as the panel shows it.
  private static func logOut(_ snapshot: PanelSnapshot) -> PanelLogOut {
    let mode = snapshot.status.overview.mode
    return switch snapshot.logOutStage {
    case .idle: .none
    case .confirming: .confirming(isEnabled: !snapshot.working && mode.allowsLogOut)
    case .held: .held(isEnabled: !snapshot.working && mode.allowsForcedLogOut)
    }
  }

  /// The menu rows: Open at Login when the app carries the login item; Recover unless the card offers it or logged
  /// out; Log Out Helper… unless logged out, waiting while a command runs, the mode forbids it or the person is
  /// asked.
  ///
  /// - Parameter snapshot: What the controller holds.
  /// - Returns: The menu.
  private static func menu(_ snapshot: PanelSnapshot) -> PanelMenu {
    let mode = snapshot.status.overview.mode
    let opensAtLogin = snapshot.opensAtLogin.flatMap { $0.isAvailable ? $0.isEnabled : nil }
    let recover: PanelMenuItem =
      if offersRecovery(snapshot.status.overview) || mode == .loggedOut {
        .hidden
      } else if snapshot.working {
        .disabled
      } else {
        .enabled
      }
    let asking = snapshot.logOutStage == .confirming
    let logOut: PanelMenuItem =
      if mode == .loggedOut {
        .hidden
      } else if !snapshot.working && mode.allowsLogOut && !asking {
        .enabled
      } else {
        .disabled
      }
    return PanelMenu(opensAtLogin: opensAtLogin, recover: recover, logOut: logOut)
  }
}
