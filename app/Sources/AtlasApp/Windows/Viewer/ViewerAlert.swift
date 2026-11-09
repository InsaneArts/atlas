/// An alert the person asked for in the main window, over the recovery alert the helper's state calls for.
enum ViewerAlert: Equatable {
  /// One of the helper's alerts, asked as every window asks it.
  case helper(HelperAlert)
  /// Restart the helper?
  case restart
  /// Clean up a session's work? Asked once what it takes was read.
  ///
  /// - Parameters:
  ///   - session: The session's key, whose work Clean Up cleans up.
  ///   - steps: What it does, a line each, such as "Quits Probe".
  ///   - warning: That the session may still be working; nil when it is not.
  case cleanup(session: String, steps: [String], warning: String?)
  /// A site joined the accounts, after Done.
  ///
  /// - Parameters:
  ///   - site: The site.
  ///   - added: Whether it joined now, rather than being listed already.
  case signedIn(site: String, added: Bool)
  /// Remove the site from the accounts too? Asked after Forget This Site… or Reset the Helper's Browser…, Keep the
  /// default.
  ///
  /// - Parameters:
  ///   - site: The site.
  ///   - browser: The browser it was seen in.
  case removeAfter(site: String, browser: String?)
}
