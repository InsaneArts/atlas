import AppKit
import AtlasKit
import SwiftUI

/// Every onboarding screen in a fixed state, by file name, for checking
/// against the design without a real setup.
@MainActor
func onboardingScreens() -> [(name: String, model: OnboardingModel, size: NSSize)] {
    func facts(_ reply: [String: Any]) -> OnboardingFacts {
        let data = (try? JSONSerialization.data(withJSONObject: reply)) ?? Data()
        return OnboardingFacts(json: data) ?? OnboardingFacts()
    }
    func controller(_ helper: [String: Any]?, finished: Bool = false) -> OnboardingFacts {
        var reply: [String: Any] = [
            "role": "controller", "me": ["name": "theiskaa", "realName": "Ismayil"],
            "app": ["installed": true, "signed": true, "linked": true],
            "configured": helper != nil, "finished": finished,
            "candidates": [["name": "testaccount", "realName": "Test Account"]],
        ]
        if let helper { reply["helper"] = helper }
        return facts(reply)
    }
    func helper(
        signedIn: Bool = true, answering: Bool = true, hidden: Bool = true, allowed: Bool = true
    ) -> [String: Any] {
        [
            "exists": true, "name": "atlas", "realName": "Atlas", "administrator": false,
            "leftoverSession": false, "paired": true, "signedIn": signedIn, "answering": answering,
            "onConsole": !hidden, "hidden": hidden,
            "permissions": ["screenCapture": allowed, "accessibility": allowed],
        ]
    }
    func asker(running: Bool, ax: Bool, screen: Bool) -> OnboardingFacts {
        facts([
            "role": "helper", "me": ["name": "atlas", "realName": "Atlas"],
            "app": ["installed": true, "signed": true],
            "helper": [
                "controller": ["uid": 501, "name": "theiskaa", "realName": "Ismayil"],
                "installed": running, "running": running,
                "permissions": ["screenCapture": screen, "accessibility": ax],
            ],
        ])
    }
    var started = OnboardingChoices()
    started.started = true
    var creating = started
    creating.creating = true
    var agentsDone = OnboardingChoices()
    agentsDone.agentsDone = true

    let card = OnboardingWindow.card
    let choose = OnboardingModel.still(controller(nil), started)
    choose.picked = ""
    let agents = OnboardingModel.still(controller(helper(), finished: true))
    agents.agents = [
        .init(id: "claude", title: "Claude Code", found: true, connected: false, on: true),
        .init(id: "codex", title: "Codex", found: true, connected: false, on: true),
    ]
    agents.projects = [URL(fileURLWithPath: "/Users/me/dev/Opie")]
    return [
        ("01-welcome", .still(controller(nil)), card),
        ("02-choose", choose, card),
        ("03-create", .still(controller(nil), creating), card),
        (
            "03-create-working",
            .still(controller(nil), creating, working: "Creating the account… Confirm in the macOS dialog."), card
        ),
        ("04-switch", .still(controller(helper(signedIn: false, answering: false, allowed: false))), card),
        ("04-switch-open-atlas", .still(controller(helper(answering: false, allowed: false))), card),
        ("05-finishing", .still(controller(helper()), finish: .running), card),
        ("05-finishing-passed", .still(controller(helper()), finish: .passed), card),
        ("06-agents", agents, card),
        ("07-done", .still(controller(helper(), finished: true), agentsDone), card),
        ("08-helper-allow", .still(asker(running: false, ax: false, screen: false)), card),
        ("09-helper-permissions", .still(asker(running: true, ax: true, screen: false)), card),
        (
            "10-helper-guide", .still(asker(running: true, ax: false, screen: false), guiding: .accessibility),
            OnboardingWindow.side
        ),
        ("11-helper-done", .still(asker(running: true, ax: true, screen: true)), card),
    ]
}

/// Draw every onboarding screen to PNGs in `directory` at Retina scale.
@MainActor
public func renderOnboarding(to directory: String) -> Bool {
    let folder = URL(fileURLWithPath: directory)
    return onboardingScreens().map { screen in
        draw(
            OnboardingView(model: screen.model), size: screen.size,
            to: folder.appendingPathComponent("\(screen.name).png"))
    }.allSatisfy { $0 }
}
