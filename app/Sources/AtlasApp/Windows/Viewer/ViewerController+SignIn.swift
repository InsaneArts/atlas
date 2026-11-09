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
  ///   - purpose: What it is for.
  ///   - site: The site.
  ///   - browser: The browser to open; the helper's own when nil.
  /// - Returns: Why it couldn't start, in the service's words; nil when it did.
  func startSignIn(_ purpose: SignInPurpose, site: String, browser: String?) async -> String? {
    guard session.signIn == nil else { return SignInWords.underWay }
    if session.place != .screen { show(.screen) }
    // The person signs in on a live picture: a paused one resumes, and input goes once a fresh one arrives.
    if session.paused { togglePause() }
    if holdingControl || control.acquiring { control.releaseControl() }
    // Reserved before the first wait, as an acquisition is: closing the window from here on calls the start off, and
    // a grant that comes after is cancelled before the control work ends, so nothing waiting on it sees a hold nobody
    // shows.
    let generation = control.beginSignInStart()
    // Every start waits for the app's control work first: a release or a Cancel still unanswered is sent again, so no
    // earlier hold is left standing at the service.
    if let refused = await actions.barrier.settle() {
      control.signInStartEnded(generation)
      return refused
    }
    guard control.wantsSignIn(generation) else {
      control.signInStartEnded(generation)
      return SignInWords.calledOff
    }
    do {
      let started = try await actions.barrier.tracking { () async throws -> SignInStarted? in
        let started: SignInStarted
        do {
          started = try await service.startSignIn(purpose.kind, site: site, browser: browser)
        } catch {
          // Only the service's own refusal, which carries its code, says no hold began; a lost reply may have started
          // one, held until its lease can no longer be alive, as a lost acquisition is.
          if (error as? ServiceError)?.code == nil { actions.barrier.holdLostAcquisition() }
          throw error
        }
        guard control.wantsSignIn(generation) else {
          _ = await actions.barrier.cancelSignInNow(started.lease)
          return nil
        }
        return started
      }
      guard let started else {
        control.signInStartEnded(generation)
        return SignInWords.calledOff
      }
      session.signIn = ViewerSignIn(purpose: purpose, site: started.site, browser: started.browser)
      session.signInEnding = false
      control.adoptSignIn(started.lease)
      return nil
    } catch {
      control.signInStartEnded(generation)
      return error.localizedDescription
    }
  }

  /// Done: the hold ends, then the window says what happened: a site signed in to joined the accounts; after
  /// Forget or Reset, whether to take the site off the list too.
  func finishSignIn() async {
    guard let signIn = session.signIn, let lease = control.signInLease, !session.signInEnding else { return }
    let generation = control.controlGeneration
    // Whether this Done still speaks for the window: nothing ended its sign-in or started another meanwhile.
    let current = { [weak self] in
      self?.control.controlGeneration == generation && self?.control.signInLease == lease
    }
    session.signInEnding = true
    defer { if current() { session.signInEnding = false } }
    // No input goes from here, and Done goes once the person's last input, a held button's release included, has
    // been answered, the lease renewed meanwhile; then Done owns it: no renewal runs, and none under way can end the
    // sign-in. One ended while the input settled, by a failed renewal or the window closing, has nothing to finish.
    guard await control.beginFinishing(lease), current() else { return }
    do {
      // Counted as control work, as an acquisition is: nothing that needs the helper free runs until it answers.
      let ended = try await actions.barrier.tracking { try await service.finishSignIn(lease: lease) }
      guard current() else { return }
      control.endSignIn()
      session.signInEnding = false
      session.signIn = nil
      if signIn.purpose.asksToRemove {
        session.alert = .removeAfter(site: ended.site, browser: ended.browser ?? signIn.browser)
      } else {
        session.alert = .signedIn(
          site: ended.site,
          browser: ended.browser ?? signIn.browser,
          added: ended.added != false
        )
      }
    } catch {
      guard current() else { return }
      session.signInEnding = false
      // The hold may still stand: it is cancelled as control ending on purpose cancels it, kept until answered.
      control.releaseControl(error.localizedDescription)
    }
    await readingLoop.readStatus()
    readingLoop.refresh()
  }

  /// Cancel: the hold ends here at once and nothing is added; the Cancel is counted as control work and sent again
  /// before anything that needs the helper free, should it fail.
  func cancelSignIn() async {
    guard session.signIn != nil, control.signInLease != nil, !session.signInEnding else { return }
    control.releaseControl()
    _ = await actions.barrier.settle()
    await readingLoop.readStatus()
    readingLoop.refresh()
  }

  /// The sign-in ended here, as its lease was lost or control was ended on purpose: the bar goes, and the notice says
  /// why when there is something to say.
  ///
  /// - Parameter reason: Why, in the service's words; nil says nothing.
  func signInEnded(_ reason: String?) {
    session.signIn = nil
    if let reason { say(reason) }
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
