import AtlasService
import AtlasUI

/// Sign In to a Site…, and Sign Out…, Forget… and Reset… on an opened account, from the Accounts pane: each starts a held
/// sign-in in the main window, which comes forward on the Screen with control, agents paused and the screen hidden
/// from them. Sign In asks for the site first in its sheet, which shows why Atlas couldn't start, if it couldn't;
/// Forget and Reset start at once for the account's site, a refusal said under the list.
extension SettingsController {
  /// Opens Sign In to a Site…, the site typed in already for an account's row.
  ///
  /// - Parameter site: The site; nil for none.
  func askSignIn(_ site: String?) {
    signIn = SignInSheetState(site: site ?? "")
  }

  /// The person typed in the sheet's field: the site changes and a refusal goes.
  ///
  /// - Parameter site: The site as typed.
  func editSignInSite(_ site: String) {
    guard signIn?.isWorking == false else { return }
    signIn?.site = site
    signIn?.refusal = nil
  }

  /// Opens an account's row to show what else can be done with it, or closes it; one opens at a time.
  ///
  /// - Parameter id: The account, as the service names it.
  func toggleAccount(_ id: String) {
    openAccount = openAccount == id ? nil : id
  }

  /// Cancel: the sheet closes, unless Continue runs.
  func cancelSignInSheet() {
    guard signIn?.isWorking != true else { return }
    signIn = nil
  }

  /// Continue: starts the sign-in in the main window; the sheet closes once it started, or says why it couldn't.
  func continueSignIn() async {
    guard let sheet = signIn, sheet.canContinue else { return }
    signIn?.isWorking = true
    signIn?.refusal = nil
    let site = sheet.site.trimmingCharacters(in: .whitespaces)
    let refusal = await dependencies.links.startSignIn(.signIn, site, nil)
    if let refusal {
      signIn?.isWorking = false
      signIn?.refusal = refusal
    } else {
      signIn = nil
    }
  }

  /// Sign Out…, Forget… or Reset… on an opened account: starts the held flow for the account's site in the browser it
  /// was seen in; a refusal says why under the list.
  ///
  /// - Parameters:
  ///   - purpose: Sign out, forget or reset.
  ///   - id: The account, as the service names it.
  func holdFor(_ purpose: SignInPurpose, account id: String) async {
    guard
      let account = dependencies.links.status().status.overview.accounts?
        .first(where: { $0.id == id }),
      account.kind == .site
    else { return }
    say(nil, in: .accounts)
    if let refusal = await dependencies.links.startSignIn(purpose, account.key, account.browser) {
      say(refusal, in: .accounts, kind: .problem)
    }
  }
}
