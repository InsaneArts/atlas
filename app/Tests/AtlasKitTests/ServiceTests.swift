import Foundation
import Testing

@testable import AtlasKit

@Suite struct StatusTests {
    @Test func aStatusReportDecodesIntoWhatThePanelShows() throws {
        let reading = try status([
            "ready": true,
            "session": ["user": ["name": "atlastest"], "inputService": ["ok": true, "ready": true]],
            "overview": [
                "mode": "controlling", "account": "atlastest", "accountName": "Atlas Test",
                "display": ["width": 2560.0, "height": 1440.0], "pending": 0,
                "activity": [
                    ["operation": "input", "actor": "viewer", "ok": true, "actionCount": 3, "ageSeconds": 42.0]
                ],
            ],
        ])
        #expect(reading.ready)
        #expect(reading.session.userName == "atlastest")
        #expect(reading.session.inputReady)
        #expect(reading.overview.mode == .controlling)
        #expect(reading.overview.accountName == "Atlas Test")
        #expect(Words.size(reading.overview.display) == "2560 × 1440")
        #expect(Overview(reading).lastAction == "You sent 3 actions · 42 s ago")
    }

    @Test func missingOrWrongFieldsFallBackToSafeDefaults() throws {
        let empty = try status([:])
        #expect(empty.overview.mode == .unavailable)
        #expect(!empty.ready)
        #expect(Overview(empty).lastAction == "No recent actions")
        let odd = try status(["ready": "yes", "overview": ["mode": 3, "activity": "none"]])
        #expect(!odd.ready)
        #expect(odd.overview.mode == .unavailable)
        #expect(odd.overview.activity.isEmpty)
        let unknown = try status(["overview": ["mode": "dreaming"]])
        #expect(unknown.overview.mode == .unavailable, "a mode this app does not know reads as unavailable")
    }

    @Test func aFailingNativeDialogCheckNamesItsStage() throws {
        let reading = try status([
            "capabilities": [
                "nativeDialogs": [
                    "state": "failed", "reason": "No panel",
                    "checks": [["stage": "open", "passed": true], ["stage": "save", "passed": false]],
                ]
            ]
        ])
        #expect(reading.capabilities.nativeDialogs.state == "failed")
        #expect(reading.capabilities.nativeDialogs.failedAt == "save")
    }
}

@Suite struct ViewerClientTests {
    @Test(arguments: [
        ("http://127.0.0.1:8765/abc/\n", true),
        ("http://example.com:8765/abc/", false),
        ("https://127.0.0.1:8765/abc/", false),
        ("http://127.0.0.1/abc/", false),
        ("http://127.0.0.1:8765/abc", false),
    ])
    func onlyALocalCapabilityURLIsAccepted(text: String, accepted: Bool) {
        #expect((ViewerClient(text) != nil) == accepted)
    }

    @Test func requestsCarryTheServiceOrigin() throws {
        let client = try #require(ViewerClient("http://127.0.0.1:8765/abc/"))
        let request = try client.request("control/release", body: ["lease": "x"])
        #expect(request.url?.absoluteString == "http://127.0.0.1:8765/abc/control/release")
        #expect(request.value(forHTTPHeaderField: "Origin") == "http://127.0.0.1:8765")
        #expect(request.httpMethod == "POST")
    }

    @Test func inputActionsEncodeAsTheServiceTakesThem() throws {
        let actions: [InputAction] = [
            .pointer(phase: .down, x: 1, y: 2, button: .left, count: 1, modifiers: ["shift"]),
            .type(text: "hi"), .key("cmd+v"), .wait(milliseconds: 50),
        ]
        let json = try #require(
            try JSONSerialization.jsonObject(with: JSONEncoder().encode(actions)) as? [[String: Any]])
        #expect(json[0]["type"] as? String == "pointer")
        #expect(json[0]["phase"] as? String == "down")
        #expect(json[1]["text"] as? String == "hi")
        #expect(json[2]["key"] as? String == "cmd+v")
        #expect(json[3]["ms"] as? Int == 50)
        #expect(InputAction.move(x: 0, y: 0).isMove)
        #expect(!InputAction.key("a").isMove)
    }
}
