#if DEBUG
  import AtlasCommands
  import AtlasOnboarding
  import AtlasOnboardingTesting
  import AtlasUI
  import Foundation
  import Testing

  @testable import AtlasApp

  /// The setup window's samples are written out by hand; these keep each one what the window would show for the same
  /// facts.
  @Suite("PreviewStates+Onboarding")
  struct OnboardingPreviewStatesTests {
    /// The person's account, with one standard account that could be the helper, and the helper `atlas` as given.
    ///
    /// - Parameters:
    ///   - helper: The helper's side of the reply, or nil before one is chosen.
    ///   - finished: Whether setup has finished.
    ///   - installed: Whether Atlas.app is where it must be.
    ///   - candidates: The standard accounts that could be the helper, by short and full name.
    ///   - onlyChoice: Whether the helper chosen before was prepared, so it is the one choice.
    ///   - leftovers: The user IDs of deleted accounts whose logins are still open.
    /// - Returns: The facts.
    /// - Throws: When the reply is not a status.
    func person(
      _ helper: [String: Any]? = nil,
      finished: Bool = false,
      installed: Bool = true,
      candidates: [(String, String)] = [("testaccount", "Test Account")],
      onlyChoice: Bool = false,
      leftovers: [Int] = []
    ) throws -> OnboardingFacts {
      var reply: [String: Any] = [
        "ok": true, "role": "controller", "me": ["name": "me"], "app": ["installed": installed, "signed": true],
        "configured": helper != nil, "finished": finished, "onlyChoice": onlyChoice,
        "candidates": candidates.map { ["name": $0.0, "realName": $0.1] },
        "leftoverLogins": leftovers.map { ["uid": $0, "kind": "orphan"] },
      ]
      if let helper { reply["helper"] = helper }
      return try facts(reply)
    }

    /// The helper `atlas`, chosen and paired, in the state given.
    ///
    /// - Parameters:
    ///   - signedIn: Whether it is logged in.
    ///   - answering: Whether Atlas answers in it.
    ///   - allowed: Whether both permissions are on.
    ///   - paired: Whether this account asked it.
    ///   - realName: Its full name.
    /// - Returns: The helper's side of the reply.
    func helper(
      signedIn: Bool = true,
      answering: Bool = true,
      allowed: Bool = true,
      paired: Bool = true,
      realName: String = "Atlas"
    ) -> [String: Any] {
      [
        "exists": true, "name": "atlas", "realName": realName, "administrator": false, "paired": paired,
        "forgettable": paired && !answering,
        "signedIn": signedIn, "answering": answering, "onConsole": false, "hidden": true,
        "permissions": ["screenCapture": allowed, "accessibility": allowed],
      ]
    }

    /// The helper's account, the account Atlas asked by Ismayil, with Atlas running there as given.
    ///
    /// - Parameters:
    ///   - running: Whether Atlas's job runs here.
    ///   - accessibility: Whether Accessibility is on.
    ///   - screen: Whether Screen Recording is on.
    ///   - me: This account's full name.
    ///   - asker: The asking account's full name.
    /// - Returns: The facts.
    /// - Throws: When the reply is not a status.
    func helperSide(
      running: Bool = true,
      accessibility: Bool = false,
      screen: Bool = false,
      me: String = "Atlas",
      asker: String = "Ismayil"
    ) throws -> OnboardingFacts {
      try facts([
        "ok": true, "role": "helper", "me": ["name": "atlas", "realName": me],
        "app": ["installed": true, "signed": true],
        "helper": [
          "controller": ["uid": 501, "name": "theiskaa", "realName": asker], "installed": running,
          "running": running, "permissions": ["screenCapture": screen, "accessibility": accessibility],
        ],
      ])
    }

    /// A snapshot that has read `facts`, past the welcome when `started`.
    ///
    /// - Parameters:
    ///   - facts: The facts read.
    ///   - started: Whether the person pressed Get Started.
    /// - Returns: The snapshot.
    func loaded(_ facts: OnboardingFacts, started: Bool = false) -> OnboardingSnapshot {
      var snapshot = OnboardingSnapshot(facts: facts, loaded: true, isOnConsole: true)
      snapshot.choices.started = started
      // The samples name the permissions as macOS 26 does.
      snapshot.macOS = OperatingSystemVersion(majorVersion: 26, minorVersion: 0, patchVersion: 0)
      return snapshot
    }

    @Test("each setup window sample is what the window shows for the same facts")
    func samplesMatchThePresenter() throws {
      var choose = loaded(try person(), started: true)
      choose.picked = .newAccount
      var many = loaded(
        try person(candidates: [
          ("testaccount", "Test Account"), ("kid", "Guest Kid"),
          ("maria", "Maria Fernanda Montgomery-Castellanos de la Vega"), ("studio", "Studio"),
        ]),
        started: true
      )
      many.picked = .newAccount
      var chooseLong = loaded(
        try person(candidates: [
          (
            "mariafernanda",
            "Maria Fernanda Montgomery-Castellanos de la Vega y Rodríguez-Hernández del Valle Alto de San Sebastián y Monteverde de los Ríos"
          ),
          ("testaccount", "Test Account"),
        ]),
        started: true
      )
      chooseLong.picked = .existing("mariafernanda")
      var chooseProblem = loaded(
        try person(candidates: [("testaccount", "Test Account"), ("kid", "Guest Kid")]),
        started: true
      )
      chooseProblem.picked = .existing("kid")
      chooseProblem.problem = "macOS could not say whether kid is an administrator."
      chooseProblem.problemAbout = "kid"
      var only = loaded(
        try person(
          helper(signedIn: false, answering: false, allowed: false, paired: false),
          candidates: [("atlas", "Atlas")],
          onlyChoice: true
        )
      )
      only.picked = .existing("atlas")
      var switchLong = loaded(
        try person(
          helper(
            signedIn: false,
            answering: false,
            allowed: false,
            realName: "Maria Fernanda Montgomery-Castellanos de la Vega"
          )
        )
      )
      switchLong.problem = OnboardingWords.lockFailed("Maria Fernanda Montgomery-Castellanos de la Vega")
      var loadingFailed = OnboardingSnapshot()
      loadingFailed.problem = PreviewStates.onboardingLoadingProblem
      var create = choose
      create.choices.creating = true
      var badName = create
      badName.form = PreviewStates.onboardingFormBadName
      var creating = create
      creating.working = OnboardingWords.creating
      var finishing = loaded(try person(helper()))
      finishing.finish = .running
      var passed = finishing
      passed.finish = .passed
      passed.passSettled = true
      var failed = finishing
      failed.finish = .failed("The test screenshot never arrived.")
      var agents = loaded(try person(helper(), finished: true))
      agents.agents = AgentID.allCases
        .map { id in
          var agent = AgentChoice(id: id)
          agent.checked = true
          agent.found = true
          agent.on = true
          agent.parts = id == .claude ? [id.rawValue, AgentsStatus.claudeAppID] : [id.rawValue]
          return agent
        }
      agents.agentsLoaded = true
      var claudeCode = agents
      claudeCode.agents[0].parts = [AgentID.claude.rawValue]
      var claudeApp = agents
      claudeApp.agents[0].parts = [AgentsStatus.claudeAppID]
      var claudeFailure = agents
      claudeFailure.agents[0].hasConnection = true
      claudeFailure.agents[0].problem = "The Claude app: could not write settings"
      var notChecked = loaded(try person(helper(), finished: true))
      notChecked.agents = AgentChoice.all
      notChecked.agentsFailed = true
      notChecked.problem = OnboardingWords.agentsCheckFailed
      var done = loaded(try person(helper(), finished: true))
      done.choices.agentsDone = true
      var allowLong = loaded(
        try helperSide(
          running: false,
          me: "Maria Fernanda Montgomery-Castellanos de la Vega",
          asker: "Christopher Alexander Montgomery-Smith"
        )
      )
      allowLong.problem = "Atlas could not finish setting up here: the login item could not be added."
      var panel = loaded(try helperSide(accessibility: true))
      panel.guiding = .screenRecording
      var panelAllowed = loaded(try helperSide(accessibility: true))
      panelAllowed.guiding = .accessibility
      let close = loaded(try person(helper(), leftovers: [502]))
      var closing = close
      closing.working = OnboardingWords.closing
      var cancelled = close
      cancelled.problem = "It’s still open. Close it before you switch."
      var closed = loaded(try person())
      closed.choices.closedLeftovers = true
      var checkingFacts = try helperSide()
      checkingFacts.asker.permissionsKnown = false
      let presented: [String: OnboardingSnapshot] = [
        "Loading": OnboardingSnapshot(),
        "Loading, failed": loadingFailed,
        "Welcome": loaded(try person()),
        "Choose": choose,
        "Choose, many accounts": many,
        "Choose, a long name": chooseLong,
        "Choose, a problem": chooseProblem,
        "Choose, only the helper set up": only,
        "Create": create,
        "Create, working": creating,
        "Create, a bad account name": badName,
        "Switch": loaded(try person(helper(signedIn: false, answering: false, allowed: false))),
        "Switch, open Atlas": loaded(try person(helper(answering: false, allowed: false))),
        "Switch, a long name": switchLong,
        "Finishing": finishing,
        "Finishing, passed": passed,
        "Finishing, failed": failed,
        "Agents": agents,
        "Agents, not checked": notChecked,
        "Claude Code only": claudeCode,
        "Claude app only": claudeApp,
        "Claude connection failed": claudeFailure,
        "Done": done,
        "Helper allow": loaded(try helperSide(running: false)),
        "Helper allow, a long name": allowLong,
        "Helper permissions, none yet": loaded(try helperSide()),
        "Helper permissions": loaded(try helperSide(accessibility: true)),
        "Helper permissions, checking": loaded(checkingFacts),
        "Helper permission panel, allowed": panelAllowed,
        "Helper permission panel": panel,
        "Helper done": loaded(try helperSide(accessibility: true, screen: true)),
        "Blocked": loaded(try person(installed: false)),
        "Close": close,
        "Close, closing": closing,
        "Close, cancelled": cancelled,
        "Close, closed": closed,
      ]
      #expect(
        Set(presented.keys) == Set(PreviewStates.onboarding.map(\.name)),
        "every sample has its facts, and no more"
      )
      for preview in PreviewStates.onboarding {
        let snapshot = try #require(presented[preview.name], "\(preview.name) has no facts")
        #expect(preview.value == OnboardingPresenter.state(snapshot), "\(preview.name)")
      }
    }
  }
#endif
