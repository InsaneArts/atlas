import Foundation
import XCTest

@testable import AtlasKit

final class OnboardingTests: XCTestCase {
    func controller(_ helper: [String: Any]? = nil, finished: Bool = false) -> OnboardingFacts {
        var reply: [String: Any] = [
            "ok": true, "role": "controller",
            "me": ["name": "theiskaa", "realName": "Ismayil"],
            "app": ["installed": true, "signed": true, "linked": true],
            "configured": helper != nil, "finished": finished,
            "candidates": [["name": "qa", "realName": "QA Account"]],
        ]
        if let helper { reply["helper"] = helper }
        return OnboardingFacts(reply)
    }

    func helper(
        signedIn: Bool = true, answering: Bool = true, hidden: Bool = true,
        allowed: Bool = true, admin: Bool = false, leftover: Bool = false
    ) -> [String: Any] {
        [
            "exists": true, "name": "atlas", "realName": "Atlas Helper",
            "administrator": admin, "leftoverSession": leftover, "paired": true,
            "signedIn": signedIn, "answering": answering, "onConsole": !hidden, "hidden": hidden,
            "permissions": ["screenCapture": allowed, "accessibility": allowed],
        ]
    }

    func testAFreshMacStartsWithWelcomeThenTheAccountChoice() {
        let facts = controller()
        var choices = OnboardingChoices()
        XCTAssertEqual(onboardingStep(facts, choices), .welcome)
        choices.started = true
        XCTAssertEqual(onboardingStep(facts, choices), .chooseHelper)
        choices.creating = true
        XCTAssertEqual(onboardingStep(facts, choices), .createAccount)
        XCTAssertEqual(facts.candidates, [.init(name: "qa", realName: "QA Account")])
    }

    func testTheSwitchScreenWaitsUntilTheHelperIsAllowedAndBack() {
        let none = OnboardingChoices()
        let signedOut = helper(signedIn: false, answering: false, allowed: false)
        XCTAssertEqual(onboardingStep(controller(signedOut), none), .switchToHelper)
        XCTAssertEqual(switchRows(controller(signedOut).helper), [.current, .waiting, .waiting])
        let notOpened = helper(answering: false, allowed: false)
        XCTAssertEqual(switchRows(controller(notOpened).helper), [.done, .current, .waiting])
        let stillThere = helper(hidden: false, allowed: true)
        XCTAssertEqual(onboardingStep(controller(stillThere), none), .switchToHelper)
        XCTAssertEqual(switchRows(controller(stillThere).helper), [.done, .done, .done])
        let notAllowed = helper(allowed: false)
        XCTAssertEqual(switchRows(controller(notAllowed).helper), [.done, .done, .current])
        XCTAssertEqual(onboardingStep(controller(helper()), none), .finishing)
    }

    func testAChoiceCutShortOffersTheChoiceAgain() {
        var unpaired = helper(signedIn: false, answering: false, allowed: false)
        unpaired["paired"] = false
        XCTAssertEqual(onboardingStep(controller(unpaired), OnboardingChoices()), .chooseHelper)
        // A helper that already answers (set up in a terminal) needs no request.
        var running = helper()
        running["paired"] = false
        XCTAssertEqual(onboardingStep(controller(running), OnboardingChoices()), .finishing)
    }

    func testAFinishedSetupGoesToTheAgentsThenDone() {
        var choices = OnboardingChoices()
        let facts = controller(helper(), finished: true)
        XCTAssertEqual(onboardingStep(facts, choices), .agents)
        choices.agentsDone = true
        XCTAssertEqual(onboardingStep(facts, choices), .done)
    }

    func testProblemsOnlyThePersonCanFixBlockTheWay() {
        let none = OnboardingChoices()
        guard case .blocked = onboardingStep(controller(helper(admin: true)), none) else {
            return XCTFail("an administrator helper is refused")
        }
        guard case .blocked = onboardingStep(controller(helper(leftover: true)), none) else {
            return XCTFail("a leftover session asks for a restart")
        }
        let unsigned = OnboardingFacts(["role": "controller", "app": ["installed": true, "signed": false]])
        guard case .blocked = onboardingStep(unsigned, none) else {
            return XCTFail("an unsigned app is refused")
        }
    }

    func testAHelperWhoseAccountWasDeletedChoosesAgain() {
        var choices = OnboardingChoices()
        let facts = controller(["exists": false])
        XCTAssertEqual(onboardingStep(facts, choices), .chooseHelper, "no welcome again")
        choices.creating = true
        XCTAssertEqual(onboardingStep(facts, choices), .createAccount)
    }

    func testTheHelperSideAllowsThenAsksForBothPermissions() {
        func facts(installed: Bool, running: Bool, ax: Bool, screen: Bool) -> OnboardingFacts {
            OnboardingFacts([
                "role": "helper",
                "me": ["name": "atlas"],
                "app": ["installed": true, "signed": true],
                "helper": [
                    "controller": ["name": "theiskaa", "realName": "Ismayil"],
                    "installed": installed, "running": running,
                    "permissions": ["screenCapture": screen, "accessibility": ax],
                ],
            ])
        }
        let none = OnboardingChoices()
        XCTAssertEqual(onboardingStep(facts(installed: false, running: false, ax: false, screen: false), none), .allow)
        XCTAssertEqual(onboardingStep(facts(installed: true, running: true, ax: false, screen: false), none), .permissions)
        XCTAssertEqual(onboardingStep(facts(installed: true, running: true, ax: true, screen: false), none), .permissions)
        XCTAssertEqual(onboardingStep(facts(installed: true, running: true, ax: true, screen: true), none), .helperDone)
        XCTAssertEqual(facts(installed: true, running: true, ax: true, screen: true).asker.title, "Ismayil")
    }

    func testAHelperSetUpInATerminalStillChecksItsPermissions() {
        func facts(running: Bool, allowed: Bool) -> OnboardingFacts {
            OnboardingFacts([
                "role": "helper", "me": ["name": "atlas"],
                "app": ["installed": true, "signed": true],
                "helper": [
                    "controller": NSNull(), "installed": true, "running": running,
                    "permissions": ["screenCapture": allowed, "accessibility": allowed],
                ],
            ])
        }
        let none = OnboardingChoices()
        XCTAssertEqual(onboardingStep(facts(running: true, allowed: false), none), .permissions)
        XCTAssertEqual(onboardingStep(facts(running: false, allowed: false), none), .permissions)
        XCTAssertEqual(onboardingStep(facts(running: true, allowed: true), none), .helperDone)
    }

    func testDotsFollowEachSide() {
        XCTAssertEqual(OnboardingStep.welcome.dots?.count, 6)
        XCTAssertEqual(OnboardingStep.done.dots?.index, 5)
        XCTAssertEqual(OnboardingStep.helperDone.dots?.count, 3)
        XCTAssertNil(OnboardingStep.blocked("x").dots)
    }
}
