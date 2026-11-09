import AtlasService

/// The words of a held sign-in, the same in Settings and the main window: the bar over the Screen for each kind, the
/// tag in the header, and the alerts after Done.
enum SignInWords {
  /// What a second Sign In says while one is under way.
  static let underWay = "A sign-in is under way already. Finish it in Atlas's window first."
  /// What a sign-in says when Atlas's window couldn't open to hold it.
  static let noWindow = "Atlas couldn't open its window. Try again."
  /// The header's tag while agents can't see the screen.
  static let hidden = "Screen · Hidden from agents"

  /// The bar over the Screen: what to do there, and that agents are paused and can't see.
  ///
  /// - Parameters:
  ///   - kind: What the hold is for.
  ///   - site: The site.
  /// - Returns: "Signing in to github.com. Agents are paused and can't see the screen.", say.
  static func bar(_ kind: SignInKind, site: String) -> String {
    let paused = "Agents are paused and can't see the screen."
    return switch kind {
    case .signIn: "Signing in to \(site). \(paused)"
    case .forget: "Delete \(site)'s data in the browser, then press Done. \(paused)"
    case .reset: "Clear the helper's browser data, then press Done. \(paused)"
    }
  }

  /// The alert after Done: the site joined the accounts, or was listed already.
  ///
  /// - Parameters:
  ///   - site: The site.
  ///   - added: Whether it joined now.
  /// - Returns: "Added github.com to Accounts", or "github.com is in Accounts".
  static func signedIn(_ site: String, added: Bool) -> String {
    added ? "Added \(site) to Accounts" : "\(site) is in Accounts"
  }

  /// What agents may do in an account at a level, as the alert after Done says it.
  ///
  /// - Parameter level: The level.
  /// - Returns: "Agents ask you before sending there.", say.
  static func level(_ level: AccountLevel) -> String {
    switch level {
    case .lookOnly: "Agents only look there."
    case .ask: "Agents ask you before sending there."
    case .trusted: "Agents act there without asking."
    }
  }

  /// The question after Forget or Reset: "Remove github.com from Accounts too?"
  ///
  /// - Parameter site: The site.
  /// - Returns: The title.
  static func removeTitle(_ site: String) -> String {
    "Remove \(site) from Accounts too?"
  }

  /// What keeping it means.
  static let removeMessage = "Agents ask you before sending there while it stays listed."
  /// The question's action.
  static let remove = "Remove"
  /// The question's action while it runs.
  static let removing = "Removing…"
  /// The question's way out, its default.
  static let keep = "Keep"
  /// The alert's one button.
  static let ok = "OK"
}
