import AppKit
import AtlasKit
import Foundation
import Testing

@testable import AtlasUI

@MainActor
@Suite struct PanelModelTests {
    @Test func thePanelFollowsTheServiceStatus() async {
        let service = FakeService()
        service.reply = ["overview": ["mode": "recovering", "account": "atlas"]]
        service.fail = ["frame"]
        let model = PanelModel(service: service, commands: nil, loginItem: nil)
        await model.readStatus()
        await model.readFrame()
        #expect(model.mode.label == "Needs you")
        #expect(model.recovering)
        #expect(model.image == nil)
        #expect(model.placeholder == "The helper's screen is not available")
    }

    @Test func releasingControlRevokesItAndSaysSo() async {
        let service = FakeService()
        let model = PanelModel(service: service, commands: nil, loginItem: nil)
        await model.releaseControl()
        #expect(service.revoked)
        #expect(model.message == "Control released.")
        #expect(!model.working)
    }

    @Test func theHelperToSwitchToIsTheOneAtlasReports() async {
        let commands = FakeCommands([
            "switch --list": #"{"ok": true, "accounts": [{"name": "me"}, {"name": "atlas", "helper": true}]}"#,
            "switch": #"{"ok": true}"#,
        ])
        let model = PanelModel(service: FakeService(), commands: commands, loginItem: nil)
        await model.findHelper()
        #expect(model.helper == "atlas")
        await model.switchToHelper()
        #expect(commands.ran.contains(["switch", "atlas", "--yes"]))
        #expect(model.message == "Switched")
    }

    @Test func aLoggedOutHelperOffersLogInInsteadOfLogOut() async {
        let service = FakeService()
        service.reply = ["overview": ["mode": "loggedOut", "account": "atlas", "accountName": "Atlas"]]
        let commands = FakeCommands([
            "switch --list": #"{"ok": true, "accounts": [{"name": "atlas", "helper": true}]}"#,
            "switch": #"{"ok": true}"#,
        ])
        let model = PanelModel(service: service, commands: commands, loginItem: nil)
        await model.readStatus()
        await model.findHelper()
        #expect(model.loggedOut)
        #expect(model.mode.label == "Logged out")
        #expect(!model.logOutAllowed)
        await model.logIn()
        #expect(commands.ran.contains(["switch", "atlas", "--yes"]))
        #expect(model.message == "Log in to Atlas, then switch back.")
    }

    @Test func loggingOutAsksFirstAndOffersForceWhenAnAppHoldsIt() async {
        let service = FakeService()
        service.reply = ["ready": true, "overview": ["mode": "controlling", "accountName": "Atlas"]]
        let commands = FakeCommands([
            "logout": #"{"ok": false, "code": "logout-held", "error": "The helper has not logged out yet"}"#
        ])
        let model = PanelModel(service: service, commands: commands, loginItem: nil)
        await model.readStatus()
        #expect(model.logOutAllowed)
        model.askToLogOut()
        #expect(model.confirmingLogOut)
        #expect(commands.ran.isEmpty, "nothing runs before the person confirms")
        await model.logOut()
        #expect(service.log.contains("revoke"), "control ends first")
        #expect(commands.ran.last == ["logout", "--yes"])
        #expect(model.message == "The helper has not logged out yet")
        #expect(model.forceOffered)
        // The held log-out stays pending; logging out anyway still may run.
        service.reply = ["overview": ["mode": "recovering"]]
        await model.readStatus()
        #expect(model.forceOffered && model.forceAllowed)
        commands.answer("logout", #"{"ok": true, "loggedOut": true}"#)
        await model.logOut(force: true)
        #expect(commands.ran.last == ["logout", "--yes", "--force"])
        #expect(!model.forceOffered)
        #expect(model.message == "The helper is logged out.")
    }

    @Test func logOutIsOfferedOnlyWhileTheHelperAnswersAndIsSettled() async {
        let service = FakeService()
        for mode in ["agent", "recovering", "unavailable", "loggedOut"] {
            service.reply = ["overview": ["mode": mode]]
            let model = PanelModel(service: service, commands: nil, loginItem: nil)
            await model.readStatus()
            #expect(!model.logOutAllowed, "\(mode)")
        }
    }

    @Test func aConfirmationThatNoLongerAppliesIsDropped() async {
        let service = FakeService()
        let model = PanelModel(service: service, commands: nil, loginItem: nil)
        await model.readStatus()
        model.askToLogOut()
        #expect(model.confirmingLogOut)
        service.reply = ["overview": ["mode": "agent"]]
        await model.readStatus()
        #expect(!model.confirmingLogOut, "an agent started meanwhile")
    }

    @Test func logInFindsTheHelperOrSaysWhy() async {
        let model = PanelModel(service: FakeService(), commands: FakeCommands([:]), loginItem: nil)
        await model.logIn()
        #expect(model.message == "Atlas couldn't find the helper account. Open Getting Started to check it.")
    }
}
