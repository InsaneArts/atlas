extension OnboardingFacts {
  /// The reply exactly as `src/lib/onboarding.rs` writes it.
  struct Reply: Decodable {
    /// This account.
    struct Me: Decodable {
      /// Its short name.
      let name: String?
      /// Its full name, when it has one.
      let realName: String?
    }

    /// Atlas.app itself.
    struct App: Decodable {
      /// Whether it sits in Applications, where the helper can run it.
      let installed: Bool?
      /// Whether its developer signed it.
      let signed: Bool?
    }

    /// The two permissions, in the helper.
    struct Permissions: Decodable {
      /// Whether Atlas has Screen Recording there; unknown while Atlas does not answer there.
      let screenCapture: Bool?
      /// Whether Atlas has Accessibility there; unknown while Atlas does not answer there.
      let accessibility: Bool?
    }

    /// In the helper: the account that asked.
    struct Controller: Decodable {
      /// Its user ID.
      let uid: Int?
      /// Its short name.
      let name: String?
      /// Its full name, when it has one.
      let realName: String?
    }

    /// The helper side: the helper as seen from the person's account, or this account as the helper.
    struct Side: Decodable {
      /// Whether the chosen account still exists.
      let exists: Bool?
      /// Its short name.
      let name: String?
      /// Its full name, when it has one.
      let realName: String?
      /// Whether it administers this Mac.
      let administrator: Bool?
      /// Whether macOS still runs a session of a deleted account with its user ID.
      let leftoverSession: Bool?
      /// This account asked this helper: its request names it, or a setup with it is recorded.
      let paired: Bool?
      /// This account asked it, and it never answered: the request may be taken back to choose another account.
      let forgettable: Bool?
      /// Whether it is logged in.
      let signedIn: Bool?
      /// Whether Atlas answers in it.
      let answering: Bool?
      /// Whether its session is the one on screen.
      let onConsole: Bool?
      /// Whether its session is off screen, as Atlas needs it.
      let hidden: Bool?
      /// Atlas's two permissions there.
      let permissions: Permissions?
      /// In the helper: the account that asked; nil when `atlas init` set it up.
      let controller: Controller?
      /// In the helper: whether Atlas's job is installed for the account asking now.
      let installed: Bool?
      /// In the helper: whether Atlas's job runs.
      let running: Bool?
    }

    /// Which side of setup this account is on.
    let role: Role?
    /// This account.
    let me: Me?
    /// Atlas.app itself.
    let app: App?
    /// Whether a helper has been chosen.
    let configured: Bool?
    /// The accounts that could be the helper.
    let candidates: [Candidate]?
    /// The helper chosen before was prepared for this account, so it is the one account offered.
    let onlyChoice: Bool?
    /// Whether setup has finished.
    let finished: Bool?
    /// Open at Login was decided once already, so the person's choice stands.
    let menubarLogin: Bool?
    /// The helper, as this account sees it, or this account as the helper.
    let helper: Side?
  }
}
