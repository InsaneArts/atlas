import AtlasService
import AtlasUI

extension ViewerPresenter {
  /// The header with "Screen · Hidden from agents" at its end while a held sign-in hides the screen from agents: this
  /// window's, or one the status reports.
  ///
  /// - Parameters:
  ///   - header: The header.
  ///   - snapshot: What the controller holds.
  /// - Returns: The header, tagged while the screen is hidden.
  static func tagged(_ header: ViewerHeaderState, _ snapshot: ViewerSnapshot) -> ViewerHeaderState {
    var tagged = header
    let hidden = snapshot.session.signIn != nil || snapshot.reading.status.overview.signingIn != nil
    tagged.tag = hidden ? SignInWords.hidden : nil
    return tagged
  }
}

extension ViewerPresenter {
  /// The level of the site's entry for the browser a held flow used: an entry is a site in one browser, so another
  /// browser's entry for the same site never speaks for it. With no browser known, the site's strictest entry.
  ///
  /// - Parameters:
  ///   - site: The site.
  ///   - browser: The browser the flow used; nil when unknown.
  ///   - overview: The status's overview, which lists the accounts.
  /// - Returns: The level, Ask Before Sending, a new entry's, when none is listed.
  static func entryLevel(site: String, browser: String?, overview: ViewerStatus.Overview) -> AccountLevel {
    let entries = (overview.accounts ?? [])
      .filter { $0.kind == .site && $0.key == site }
    if let browser {
      return entries.first { $0.browser == browser }?.level ?? .ask
    }
    let strictest: [AccountLevel] = [.lookOnly, .ask, .trusted]
    return strictest.first { level in entries.contains { $0.level == level } } ?? .ask
  }
}
