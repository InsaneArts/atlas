extension OnboardingFacts {
  /// The reply exactly as `src/lib/onboarding.rs` writes it.
  struct Reply: Decodable {
    /// This account.
    struct Me: Decodable {
      let name: String?
      let realName: String?
    }

    /// Atlas.app itself.
    struct App: Decodable {
      let installed: Bool?
      let signed: Bool?
      let error: String?
    }

    /// The two permissions, in the helper.
    struct Permissions: Decodable {
      let screenCapture: Bool?
      let accessibility: Bool?
    }

    /// In the helper: the account that asked.
    struct Controller: Decodable {
      let uid: Int?
      let name: String?
      let realName: String?
    }

    /// The helper side: the helper as seen from the person's account, or this account as the helper.
    struct Side: Decodable {
      let exists: Bool?
      let name: String?
      let realName: String?
      let administrator: Bool?
      let leftoverSession: Bool?
      let paired: Bool?
      let signedIn: Bool?
      let answering: Bool?
      let onConsole: Bool?
      let hidden: Bool?
      let permissions: Permissions?
      let controller: Controller?
      let installed: Bool?
      let running: Bool?
    }
    let role: Role?
    let me: Me?
    let app: App?
    let configured: Bool?
    let candidates: [Candidate]?
    let finished: Bool?
    let menubarLogin: Bool?
    let helper: Side?
  }
}
