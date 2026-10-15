import Foundation
import Testing

@testable import AtlasKit

@Suite struct WordsTests {
    @Test(arguments: [
        (2.0, "just now"), (42, "42 s ago"), (600, "10 min ago"), (7200, "2 h ago"),
    ])
    func agesReadAsWords(seconds: Double, words: String) {
        #expect(Words.ago(seconds) == words)
    }

    @Test func operationsReadAsWords() {
        #expect(Words.describe(.init(operation: "run", ok: true)) == "ran a command")
        #expect(Words.describe(.init(operation: "input", ok: false)) == "input failed")
        #expect(Words.describe(.init(operation: "input", ok: true, actionCount: 1)) == "sent 1 action")
        #expect(Words.size(nil) == nil)
        #expect(Words.size(.init(width: 0, height: 10)) == nil)
    }

    @Test func everyModeHasALabelToneAndMark() {
        #expect(Mode.of(.observing).tone == .ready)
        #expect(Mode.of(.controlling).mark == .filled)
        #expect(Mode.of(.recovering).tone == .attention)
        #expect(Mode.of(.unavailable).label == "Unavailable")
    }

    @Test func aViewerKnowsWhetherItHoldsControl() {
        var overview = ViewerStatus.Overview()
        overview.mode = .controlling
        #expect(Mode.seen(by: overview, holdingControl: true).label == "You have control")
        #expect(Mode.seen(by: overview, holdingControl: false).label == "Another viewer has control")
        overview.mode = .recovering
        #expect(Mode.seen(by: overview, holdingControl: true).label == "Needs you")
    }
}

@Suite struct OverviewTests {
    @Test func captionsAreRecentAndOldestFirst() throws {
        let reading = try status([
            "overview": [
                "activity": [
                    ["operation": "input", "ok": true, "actionCount": 2, "ageSeconds": 3.0],
                    ["operation": "run", "ok": true, "ageSeconds": 20.0],
                    ["operation": "restart", "ok": true, "ageSeconds": 400.0],
                ]
            ]
        ])
        let captions = Overview(reading).captions()
        #expect(captions.map(\.line) == ["Agent ran a command", "Agent sent 2 actions"])
    }

    @Test func controlWaitsForAgentsRecoveryAndOtherViewers() throws {
        func overview(_ mode: String) throws -> Overview { Overview(try status(["overview": ["mode": mode]])) }
        #expect(try overview("agent").controlBlocked(holdingControl: false) == "An agent is acting in the helper")
        #expect(try overview("controlling").controlBlocked(holdingControl: true) == nil)
        #expect(try overview("controlling").controlBlocked(holdingControl: false) != nil)
        #expect(try overview("observing").controlBlocked(holdingControl: false) == nil)
        #expect(try overview("recovering").recoveryOffered(holdingControl: false))
        #expect(try !overview("recovering").recoveryOffered(holdingControl: true))
    }
}

@Suite struct StatusRowTests {
    @Test func unknownStaysDistinctAndNativeDialogsShowSeparately() throws {
        let reading = try status([
            "checks": [
                ["name": "Helper connection", "ok": true],
                ["name": "Input permission", "ok": NSNull(), "nextStep": "Run start-input."],
                ["name": "Screen recording", "ok": false, "nextStep": "Enable it."],
            ],
            "capabilities": [
                "nativeDialogs": [
                    "state": "failed", "ageSeconds": 300, "reason": "Native Save/Open did not complete in this session",
                    "checks": [
                        ["kind": "save", "passed": false, "stage": "save confirmation enabled"],
                        ["kind": "open", "passed": false, "stage": "open confirmation enabled"],
                    ],
                ]
            ],
        ])
        let rows = Overview(reading).statusRows
        #expect(rows.map(\.state) == [.ready, .unknown, .bad, .bad])
        #expect(rows[1].text == "Unknown. Run start-input.")
        #expect(rows[3].text.hasPrefix("Failed 5 min ago at “save confirmation enabled”"))
        let stale = try status([
            "checks": [], "capabilities": ["nativeDialogs": ["state": "stale", "reason": "Changed"]],
        ])
        #expect(Overview(stale).statusRows.first?.state == .unknown)
        #expect(Overview(try status(["checks": []])).statusRows.isEmpty)
    }

    @Test func freshnessReadsAsWords() {
        #expect(Words.freshness(1) == "Live")
        #expect(Words.freshness(5) == "Updated 5 s ago")
        #expect(Words.freshness(Freshness.stale) == "Picture is stale")
        #expect(Words.freshness(nil) == "Waiting for the first picture")
    }

    @Test func factsAndActivityReadAsWords() throws {
        let base: [String: Any] = [
            "overview": ["display": ["width": 2560, "height": 1440], "pending": 0, "mode": "observing"],
            "session": ["inputService": ["ok": true, "ready": true, "pid": 42]],
        ]
        let overview = Overview(try status(base))
        #expect(
            overview.facts(holdingControl: false).map { [$0.0, $0.1] } == [
                ["Display", "2560 × 1440"], ["Input", "Ready"], ["Control", "Available"], ["Pending actions", "None"],
            ])
        #expect(overview.facts(holdingControl: true)[2].1 == "This window")
        for (mode, expected) in [
            ("agent", "Agent working"), ("recovering", "After recovery"), ("blocked", "Not ready"),
            ("unavailable", "Unavailable"),
        ] {
            let reading = try status(["overview": ["mode": mode]])
            #expect(Overview(reading).facts(holdingControl: false)[2].1 == expected)
        }
        let items = Overview(
            try status([
                "overview": [
                    "activity": [
                        ["operation": "input", "ok": true, "actionCount": 3, "ageSeconds": 3],
                        ["operation": "run", "ok": true, "ageSeconds": 40],
                        ["operation": "restart", "ok": true, "ageSeconds": 70, "actor": "viewer"],
                        ["operation": "input", "ok": false, "actionCount": 8, "ageSeconds": 200],
                    ]
                ]
            ])
        ).activity()
        #expect(
            items.map(\.line) == ["Agent sent 3 actions", "Agent ran a command", "You recovered", "Agent input failed"])
        #expect(items.map(\.when) == ["just now", "40 s ago", "70 s ago", "3 min ago"])
    }

    @Test func controlHeldByAnotherViewerIsNeverShownAsYours() throws {
        let held = Overview(try status(["overview": ["mode": "controlling", "reason": "A viewer has control"]]))
        #expect(Mode.seen(by: held.status.overview, holdingControl: true).label == "You have control")
        #expect(Mode.seen(by: held.status.overview, holdingControl: false).label == "Another viewer has control")
        #expect(held.reason(holdingControl: false) == "Another viewer has control")
        #expect(held.reason(holdingControl: true) == "You have mouse and keyboard control")
        #expect(held.controlBlocked(holdingControl: true) == nil)
    }

    @Test func theRecoveryCardFollowsRecoveryNotAStaleStatus() throws {
        let needs = Overview(try status(["overview": ["mode": "recovering", "reason": "An outcome is unknown"]]))
        #expect(
            needs.recovery(holdingControl: false, recovering: false)
                == Recovery(title: "The last action's outcome is unknown", reason: "An outcome is unknown", busy: false)
        )
        let blocked = Overview(try status(["overview": ["mode": "blocked", "reason": "x"]]))
        #expect(blocked.recovery(holdingControl: false, recovering: false)?.title == "The helper isn't ready")
        #expect(needs.recovery(holdingControl: false, recovering: true)?.busy == true)
        let watching = Overview(try status(["overview": ["mode": "observing"]]))
        #expect(watching.recovery(holdingControl: false, recovering: true) != nil)
        #expect(watching.recovery(holdingControl: false, recovering: false) == nil)
        #expect(needs.recovery(holdingControl: true, recovering: false) == nil, "never while holding control")
    }
}
