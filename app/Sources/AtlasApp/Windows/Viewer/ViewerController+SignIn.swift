import AtlasService
import AtlasUI

/// A held sign-in in the main window: started for Settings, it holds the person's lease and the private hold while
/// they sign in, delete a site's data or clear the browser in the helper, and only Done or Cancel on the bar over the
/// Screen ends it; switching to a password manager, covering the window or leaving the Screen only stops sending input.
/// Done says what happened, and after Forget or Reset asks whether to take the site off the list too.
extension ViewerController {
  /// Starts a held sign-in on the Screen: control this window held goes back first, then the service holds the
  /// helper and opens the site, and the window holds the sign-in'"'"'s lease.
  ///
  /// - Parameters:
  ///   - kind: What it is for.
  ///   - site: The site.
  ///   - browser: The browser to open; the helper's own when nil.
  /// - Returns: Why it couldn't start, in the service's words; nil when it did.
  func startSignIn(_ kind: SignInKind, site: String, browser: String?) async -> String? {
    guard session.signIn == nil else { return SignInWords.underWay }
    if session.place != .screen { show(.screen) }
    if holdingControl || control.acquiring {
      control.releaseControl()
      if let refused = await actions.barrier.settle() { return refused }
    }
    do {
      let started = try await service.startSignIn(kind, site: site, browser: browser)
      session.signIn = ViewerSignIn(kind: started.kind, site: started.site, browser: started.browser)
      control.adoptSignIn(started.lease)
      return nil
    } catch {
      return error.localizedDescription
    }
  }

  /// Done: the hold ends, then the window says what happened: a site signed in to joined the accounts; after
  /// Forget or Reset, whether to take the site off the list too.
  func finishSignIn() async {
    guard let signIn = session.signIn, let lease = control.signInLease, !session.signInEnding else { return }
    session.signInEnding = true
    defer { session.signInEnding = false }
    do {
      let ended = try await service.finishSignIn(lease: lease)
      control.endSignIn()
      session.signIn = nil
      switch ended.kind {
      case .signIn: session.alert = .signedIn(site: ended.site, added: ended.added != false)
      case .forget, .reset: session.alert = .removeAfter(site: ended.site, browser: ended.browser ?? signIn.browser)
      }
    } catch {
      control.endSignIn()
      session.signIn = nil
      say(error.localizedDescription)
    }
    await readingLoop.readStatus()
    readingLoop.refresh()
  }

  /// Cancel: the hold ends and nothing is added.
  func cancelSignIn() async {
    guard session.signIn != nil, let lease = control.signInLease, !session.signInEnding else { return }
    session.signInEnding = true
    defer { session.signInEnding = false }
    try? await service.cancelSignIn(lease: lease)
    control.endSignIn()
    session.signIn = nil
    await readingLoop.readStatus()
    readingLoop.refresh()
  }

  /// The sign-in ended on its own, as its lease was lost: the bar goes and the notice says why.
  ///
  /// - Parameter reason: Why, in the service's words.
  func signInEnded(_ reason: String) {
    session.signIn = nil
    say(reason)
  }

  /// Takes the site a Forget or Reset was for off the accounts, as Remove from List… does; one no longer listed is
  /// left alone.
  ///
  /// - Parameters:
  ///   - site: The site.
  ///   - browser: The browser it was seen in.
  func removeAfterSignIn(site: String, browser: String?) async {
    session.alert = nil
    let listed = reading.status.overview.accounts?
      .first { $0.kind == .site && $0.key == site && ($0.browser == browser || browser == nil) }
    guard let listed else { return }
    do {
      try await service.removeAccount(listed)
    } catch {
      say(error.localizedDescription)
    }
    await readingLoop.readStatus()
  }
}
