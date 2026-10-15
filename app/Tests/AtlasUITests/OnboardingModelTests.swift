import AppKit
import AtlasKit
import Foundation
import Testing

@testable import AtlasUI

let freshMac = """
    {"ok": true, "role": "controller", "me": {"name": "me"}, "app": {"installed": true, "signed": true},
     "configured": false, "candidates": []}
    """

@MainActor
@Suite struct OnboardingModelTests {
    @Test func aFreshMacOffersANewAccountFirst() async {
        let commands = FakeCommands(["onboarding status": freshMac])
        let model = OnboardingModel(commands: commands)
        model.refresh()
        await settle { model.loaded }
        #expect(model.step == .welcome)
        #expect(model.picked == "", "no candidates: a new account is picked")
        model.begin()
        await model.useHelper()
        #expect(model.step == .createAccount)
    }

    @Test func thePasswordTravelsOnStandardInputAndIsForgotten() async {
        let commands = FakeCommands(["onboarding status": freshMac, "onboarding create": #"{"ok": true}"#])
        let model = OnboardingModel(commands: commands)
        model.password = "hunter2"
        model.verify = "hunter2"
        #expect(model.canCreate)
        await model.createAccount()
        let create = commands.ran.firstIndex { $0.starts(with: ["onboarding", "create"]) }
        #expect(create != nil)
        if let create {
            #expect(!commands.ran[create].contains("hunter2"))
            #expect(commands.inputs[create] == Data("hunter2\n".utf8))
        }
        #expect(model.password.isEmpty && model.verify.isEmpty)
    }

    @Test func passwordsMustMatchAndStayPrintable() {
        let model = OnboardingModel(commands: FakeCommands([:]))
        model.password = "one"
        model.verify = "two"
        #expect(model.passwordProblem == "The two passwords are different.")
        #expect(!model.canCreate)
        model.password = "bad\u{7}"
        #expect(model.passwordProblem == "Use a password without control characters.")
    }

    @Test func aFailedCommandIsShownInOneLine() async {
        let commands = FakeCommands(["onboarding status": freshMac])
        let model = OnboardingModel(commands: commands)
        model.picked = "qa"
        await model.useHelper()
        #expect(model.problem == "unknown command")
        #expect(model.working == nil)
    }

    @Test func aClosedWindowStartsNothingNew() async {
        let commands = FakeCommands(["onboarding status": freshMac])
        let model = OnboardingModel(commands: commands)
        await model.closing()
        model.refresh()
        try? await Task.sleep(for: .milliseconds(100))
        #expect(commands.ran.isEmpty)
        #expect(!model.loaded)
    }

    @Test func theHelperAllowsExactlyTheAccountShown() async {
        let commands = FakeCommands([
            "onboarding status": """
            {"ok": true, "role": "helper", "me": {"name": "atlas"}, "app": {"installed": true, "signed": true},
             "helper": {"controller": {"uid": 501, "name": "me"}, "installed": false, "running": false}}
            """,
            "onboarding accept": #"{"ok": true}"#,
            "onboarding window": #"{"ok": true}"#,
        ])
        let model = OnboardingModel(commands: commands)
        model.refresh()
        await settle { model.loaded }
        #expect(model.step == .allow)
        await model.allow()
        #expect(
            commands.ran.contains(["onboarding", "accept", "--controller-uid", "501", "--controller-name", "me"]))
    }
}

@MainActor
@Suite struct OnboardingRenderTests {
    @Test func everyOnboardingScreenDraws() throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(
            "atlas-render-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        #expect(renderOnboarding(to: directory.path))
        let drawn = try FileManager.default.contentsOfDirectory(atPath: directory.path)
        #expect(drawn.count == onboardingScreens().count)
        for name in drawn {
            let image = try #require(NSImage(contentsOf: directory.appendingPathComponent(name)))
            #expect(image.size.width >= OnboardingWindow.side.width)
        }
    }

    @Test func everyScreenIsTheStepItIsNamedFor() {
        let steps = Dictionary(uniqueKeysWithValues: onboardingScreens().map { ($0.name, $0.model.step) })
        #expect(steps["01-welcome"] == .welcome)
        #expect(steps["04-switch"] == .switchToHelper)
        #expect(steps["06-agents"] == .agents)
        #expect(steps["08-helper-allow"] == .allow)
        #expect(steps["11-helper-done"] == .helperDone)
    }
}
