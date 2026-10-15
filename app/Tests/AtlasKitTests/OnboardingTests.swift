import Foundation
import Testing

@testable import AtlasKit

@Suite struct OnboardingTests {
    func controller(_ helper: [String: Any]? = nil, finished: Bool = false) throws -> OnboardingFacts {
        var reply: [String: Any] = [
            "ok": true, "role": "controller",
            "me": ["name": "theiskaa", "realName": "Ismayil"],
            "app": ["installed": true, "signed": true, "linked": true],
            "configured": helper != nil, "finished": finished,
            "candidates": [["name": "qa", "realName": "QA Account"]],
        ]
        if let helper { reply["helper"] = helper }
        return try facts(reply)
    }

    func helper(
        signedIn: Bool = true, answering: Bool = true, hidden: Bool = true,
        allowed: Bool = true, admin: Bool = false, leftover: Bool = false, paired: Bool = true
    ) -> [String: Any] {
        [
            "exists": true, "name": "atlas", "realName": "Atlas",
            "administrator": admin, "leftoverSession": leftover, "paired": paired,
            "signedIn": signedIn, "answering": answering, "onConsole": !hidden, "hidden": hidden,
            "permissions": ["screenCapture": allowed, "accessibility": allowed],
        ]
    }

    func asker(
        name: String? = "theiskaa", installed: Bool, running: Bool, ax: Bool, screen: Bool
    ) throws
        -> OnboardingFacts
    {
        let controller: Any = name.map { ["uid": 501, "name": $0, "realName": "Ismayil"] } ?? NSNull()
        return try facts([
            "role": "helper", "me": ["name": "atlas"],
            "app": ["installed": true, "signed": true],
            "helper": [
                "controller": controller, "installed": installed, "running": running,
                "permissions": ["screenCapture": screen, "accessibility": ax],
            ],
        ])
    }

    @Test func aReplyThatIsNotAStatusIsRefused() {
        #expect(OnboardingFacts(json: Data("not json".utf8)) == nil)
        #expect(OnboardingFacts(json: Data("[]".utf8)) == nil)
    }

    @Test func aFreshMacStartsWithWelcomeThenTheAccountChoice() throws {
        let facts = try controller()
        var choices = OnboardingChoices()
        #expect(onboardingStep(facts, choices) == .welcome)
        choices.started = true
        #expect(onboardingStep(facts, choices) == .chooseHelper)
        choices.creating = true
        #expect(onboardingStep(facts, choices) == .createAccount)
        #expect(facts.candidates == [.init(name: "qa", realName: "QA Account")])
        #expect(facts.myName == "Ismayil")
    }

    @Test func theSwitchScreenWaitsUntilTheHelperIsAllowedAndBack() throws {
        let none = OnboardingChoices()
        let signedOut = try controller(helper(signedIn: false, answering: false, allowed: false))
        #expect(onboardingStep(signedOut, none) == .switchToHelper)
        #expect(switchRows(signedOut.helper) == [.current, .waiting, .waiting])
        #expect(
            switchRows(try controller(helper(answering: false, allowed: false)).helper) == [.done, .current, .waiting])
        let stillThere = try controller(helper(hidden: false, allowed: true))
        #expect(onboardingStep(stillThere, none) == .switchToHelper)
        #expect(switchRows(stillThere.helper) == [.done, .done, .done])
        #expect(switchRows(try controller(helper(allowed: false)).helper) == [.done, .done, .current])
        #expect(onboardingStep(try controller(helper()), none) == .finishing)
    }

    @Test func aChoiceCutShortOffersTheChoiceAgain() throws {
        let unpaired = try controller(helper(signedIn: false, answering: false, allowed: false, paired: false))
        #expect(onboardingStep(unpaired, OnboardingChoices()) == .chooseHelper)
        // A helper that already answers (set up in a terminal) needs no request.
        #expect(onboardingStep(try controller(helper(paired: false)), OnboardingChoices()) == .finishing)
    }

    @Test func aFinishedSetupGoesToTheAgentsThenDone() throws {
        var choices = OnboardingChoices()
        let facts = try controller(helper(), finished: true)
        #expect(onboardingStep(facts, choices) == .agents)
        choices.agentsDone = true
        #expect(onboardingStep(facts, choices) == .done)
    }

    @Test func problemsOnlyThePersonCanFixBlockTheWay() throws {
        let none = OnboardingChoices()
        guard case .blocked = onboardingStep(try controller(helper(admin: true)), none) else {
            Issue.record("an administrator helper is refused")
            return
        }
        guard case .blocked = onboardingStep(try controller(helper(leftover: true)), none) else {
            Issue.record("a leftover session asks for a restart")
            return
        }
        let unsigned = try facts(["role": "controller", "app": ["installed": true, "signed": false]])
        guard case .blocked = onboardingStep(unsigned, none) else {
            Issue.record("an unsigned app is refused")
            return
        }
    }

    @Test func aHelperWhoseAccountWasDeletedChoosesAgain() throws {
        var choices = OnboardingChoices()
        let facts = try controller(["exists": false])
        #expect(onboardingStep(facts, choices) == .chooseHelper, "no welcome again")
        choices.creating = true
        #expect(onboardingStep(facts, choices) == .createAccount)
    }

    @Test func theHelperSideAllowsThenAsksForBothPermissions() throws {
        let none = OnboardingChoices()
        #expect(onboardingStep(try asker(installed: false, running: false, ax: false, screen: false), none) == .allow)
        #expect(
            onboardingStep(try asker(installed: true, running: true, ax: false, screen: false), none) == .permissions)
        #expect(
            onboardingStep(try asker(installed: true, running: true, ax: true, screen: false), none) == .permissions)
        let done = try asker(installed: true, running: true, ax: true, screen: true)
        #expect(onboardingStep(done, none) == .helperDone)
        #expect(done.asker.title == "Ismayil")
        #expect(done.asker.uid == 501)
    }

    @Test func aJobThatDoesNotRunIsAllowedAgainWhenAsked() throws {
        // An old Atlas's job, installed but not running: allowing again fixes it.
        let none = OnboardingChoices()
        #expect(onboardingStep(try asker(installed: true, running: false, ax: true, screen: true), none) == .allow)
    }

    @Test func aHelperSetUpInATerminalStillChecksItsPermissions() throws {
        let none = OnboardingChoices()
        #expect(
            onboardingStep(try asker(name: nil, installed: true, running: true, ax: false, screen: false), none)
                == .permissions)
        #expect(
            onboardingStep(try asker(name: nil, installed: true, running: true, ax: true, screen: true), none)
                == .helperDone)
        // Not running, and no one asked: nothing to allow here.
        guard
            case .blocked = onboardingStep(
                try asker(name: nil, installed: true, running: false, ax: false, screen: false), none)
        else {
            Issue.record("a job that does not run, with no one asking, is explained")
            return
        }
    }

    @Test func dotsFollowEachSide() {
        #expect(OnboardingStep.welcome.dots?.count == 6)
        #expect(OnboardingStep.done.dots?.index == 5)
        #expect(OnboardingStep.helperDone.dots?.count == 3)
        #expect(OnboardingStep.blocked("x").dots == nil)
    }
}
