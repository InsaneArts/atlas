import Foundation

extension VMFacts {
  /// The VM in a status reply, exactly as `crates/cli/src/user/onboarding/status.rs` writes it.
  struct Reply: Decodable {
    /// Atlas VM's word on how far making it got.
    struct Making: Decodable {
      /// What it does now.
      let phase: Phase?
      /// The bytes of the download received.
      let received: Int64?
      /// The bytes the download has in all.
      let total: Int64?
      /// How much of the phase is done.
      let fraction: Double?
    }

    /// Which permissions the guest said it allows in the setup window.
    struct Allowed: Decodable {
      /// Screen & System Audio Recording.
      let screenCapture: Bool?
      /// Accessibility.
      let accessibility: Bool?
    }

    /// Whether the process making the add still runs.
    struct Adder: Decodable {
      /// Whether it runs; unknown when nil.
      let running: Bool?
    }

    /// Its name in the settings.
    let name: String?
    /// `adding`, `created` or `removing`.
    let lifecycle: String?
    /// The steps of its add done, by the record's names.
    let steps: [String]?
    /// How far making it got.
    let progress: Making?
    /// Where its setup stands.
    let setup: String?
    /// Which permissions are allowed in the setup window.
    let allowed: Allowed?
    /// Why its add last stopped short.
    let failure: String?
    /// The process making its add.
    let adder: Adder?
    /// Whether it is ready for agents.
    let ready: Bool?
  }

  /// The facts of the VM a reply names.
  ///
  /// - Parameter reply: The VM as the status reply wrote it.
  init(_ reply: Reply) {
    name = reply.name ?? ""
    adding = reply.lifecycle == "adding"
    steps = Set((reply.steps ?? []).compactMap(Step.init(rawValue:)))
    progress = reply.progress.map { making in
      Progress(
        phase: making.phase ?? .unknown,
        received: making.received,
        total: making.total,
        fraction: making.fraction
      )
    }
    setup = reply.setup.flatMap(Setup.init(rawValue:))
    recordingAllowed = reply.allowed?.screenCapture ?? false
    accessibilityAllowed = reply.allowed?.accessibility ?? false
    failure = reply.failure
    adderRunning = reply.adder?.running
    ready = reply.ready ?? false
  }
}
