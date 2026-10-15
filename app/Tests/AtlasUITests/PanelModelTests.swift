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
        let model = PanelModel(service: service, commands: nil, loginItem: nil)
        await model.readStatus()
        await model.readFrame()
        #expect(model.mode.label == "Needs you")
        #expect(model.recovering)
        #expect(model.image == nil)
        #expect(model.placeholder == "The helper’s screen is not available")
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
}
