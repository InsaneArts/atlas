import AtlasServiceTesting
import Foundation
import Testing

@testable import AtlasService

@Suite("ActivityLog.Entry")
struct ActivityLogEntryTests {
  @Test("an activity row with every field null reads as an agent's failed action")
  func allNull() throws {
    let log = try ViewerClient.activity(activityAllNull, response: reply(200))
    #expect(
      log.entries == [
        ActivityLog.Entry(id: "9f2c41d07a3b55e1", outcome: .failed, actor: .agent, operation: "action")
      ]
    )
  }

  @Test("an activity row's start is read as milliseconds since 1970")
  func startedAt() throws {
    let body = Data(#"{"activity": [{"id": "7", "startedAt": 1792665097600, "ageSeconds": 4}]}"#.utf8)
    let log = try ViewerClient.activity(body, response: reply(200))
    #expect(log.entries.first?.startedAt == Date(timeIntervalSince1970: 1_792_665_097.6))
  }

  @Test("an empty session key reads as Terminal's, as a missing one does, and the person's own rows have no group")
  func emptySessionKeyIsTerminal() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "actor": "agent", "session": ""},
        {"id": "2", "actor": "agent"},
        {"id": "3", "actor": "agent", "session": "claude:a1"},
        {"id": "4", "actor": "viewer"}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.session) == [nil, nil, "claude:a1", nil])
    #expect(entries.map(\.sessionKey) == ["", "", "claude:a1", ""])
    #expect(entries.map(\.groupKey) == ["", "", "claude:a1", nil])
    let scope = ActivityScope.session("")
    let terminal = entries.filter { scope.admits($0) }
    #expect(terminal.map(\.id) == ["1", "2"])
  }

  @Test("an activity row's verb, app, label, bundle and keys read as the service writes them")
  func namesWhatItDid() throws {
    let body = Data(
      #"""
      {"activity": [{"id": "1", "verb": "key", "app": "TextEdit", "label": "⇧⌘Z Page Up",
      "bundleId": "com.apple.TextEdit", "keys": [["⇧", "⌘", "Z"], ["Page Up"]]}]}
      """#.utf8
    )
    let entry = try #require(try ViewerClient.activity(body, response: reply(200)).entries.first)
    #expect(entry.verb == "key")
    #expect(entry.app == "TextEdit")
    #expect(entry.label == "⇧⌘Z Page Up")
    #expect(entry.bundleId == "com.apple.TextEdit")
    #expect(entry.keys == [["⇧", "⌘", "Z"], ["Page Up"]])
  }

  @Test("an activity row's picture before reads by its file name, and a missing, empty or odd one reads as none")
  func pictureBefore() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "before": "1792665097600000000-before.jpg"},
        {"id": "2"},
        {"id": "3", "before": null},
        {"id": "4", "before": ""},
        {"id": "5", "before": ["1-before.jpg"]}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.before) == ["1792665097600000000-before.jpg", nil, nil, nil, nil])
  }

  @Test("an activity row's picture after reads by its file name, and a missing or odd one reads as none")
  func pictureAfter() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "after": "1792665098600000000-after.jpg"},
        {"id": "2", "after": "1792665099000000000-before.jpg"},
        {"id": "3"},
        {"id": "4", "after": 7}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.after) == ["1792665098600000000-after.jpg", "1792665099000000000-before.jpg", nil, nil])
  }

  @Test("when an activity row's picture after was taken reads as milliseconds since 1970, and as none without one")
  func afterAt() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "after": "1792665098600000000-after.jpg", "afterAt": 1792665099020},
        {"id": "2", "after": "1792665098600000000-after.jpg"},
        {"id": "3", "after": "1792665098600000000-after.jpg", "afterAt": "soon"},
        {"id": "4", "afterAt": 1792665099020}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.map(\.afterAt) == [Date(timeIntervalSince1970: 1_792_665_099.02), nil, nil, nil])
  }

  @Test("an action's facts read each field on its own, and odd fields read as none without dropping the rest")
  func actionFacts() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "facts": {
          "action": "type", "outcome": "worked", "display": {"width": 1440, "height": 900},
          "point": {"x": 720.5, "y": 450}, "text": "Release 0.1.1", "length": 13, "label": "Message",
          "at": 1792665097600, "endedAt": 1792665098100
        }},
        {"id": "2", "facts": {
          "action": "drag", "outcome": "uncertain", "point": {"x": 10, "y": 20}, "to": {"x": 30, "y": "far"},
          "clicks": 2, "button": "right", "keys": [["⌘", "S"]], "scroll": {"dy": 12}, "display": {"width": 0, "height": 9}
        }},
        {"id": "3", "facts": {"action": "type", "secure": true, "text": "hunter2", "length": 7}},
        {"id": "4", "facts": {"action": "teleport", "outcome": 1, "point": [1, 2]}},
        {"id": "5", "facts": "none"},
        {"id": "6"}
      ]}
      """#.utf8
    )
    let facts = try ViewerClient.activity(body, response: reply(200))
      .entries
      .map(\.facts)
    let typed = try #require(facts[0])
    #expect(typed.action == .type)
    #expect(typed.outcome == .worked)
    #expect(typed.display == ActionFacts.Size(width: 1440, height: 900))
    #expect(typed.point == ActionFacts.Point(x: 720.5, y: 450))
    #expect(typed.text == "Release 0.1.1")
    #expect(typed.length == 13)
    #expect(typed.label == "Message")
    #expect(typed.at == Date(timeIntervalSince1970: 1_792_665_097.6))
    #expect(typed.endedAt == Date(timeIntervalSince1970: 1_792_665_098.1))

    let dragged = try #require(facts[1])
    #expect(dragged.action == .drag)
    #expect(dragged.outcome == .uncertain)
    #expect(dragged.to == nil, "an odd end reads as none")
    #expect(dragged.display == nil, "an empty display reads as none")
    #expect(dragged.clicks == 2)
    #expect(dragged.button == "right")
    #expect(dragged.keys == [["⌘", "S"]])
    #expect(dragged.scroll == ActionFacts.Scroll(dx: 0, dy: 12))

    let password = try #require(facts[2])
    #expect(password.secure)
    #expect(password.text == nil && password.length == nil, "a password's text and length are never kept")

    let odd = try #require(facts[3])
    #expect(odd.action == nil && odd.outcome == nil && odd.point == nil)
    #expect(facts[4] == nil && facts[5] == nil)
  }

  @Test("an action's element and task read when given, and an odd one reads as none without the rest")
  func actionFactsElementAndTask() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "facts": {
          "action": "click", "element": {"frame": {"x": 345.5, "y": 711, "width": 1066, "height": 135},
          "role": "AXTextArea", "name": "Message #general"}, "window": "general · InsaneArts",
          "task": "Post the 0.1.1 release note"
        }},
        {"id": "2", "facts": {
          "action": "key", "element": {"frame": {"x": 1, "y": 2, "width": 3, "height": 4}, "role": 7, "name": ""},
          "window": 3, "task": ""
        }},
        {"id": "3", "facts": {"action": "click", "element": {"frame": {"x": 1, "y": 2, "width": 0, "height": 4}}}},
        {"id": "4", "facts": {"action": "click", "element": {"role": "AXButton"}, "label": "Send"}},
        {"id": "5", "facts": {"action": "click"}}
      ]}
      """#.utf8
    )
    let facts = try ViewerClient.activity(body, response: reply(200))
      .entries
      .map(\.facts)
    let clicked = try #require(facts[0])
    #expect(
      clicked.element
        == ActionFacts.Element(
          frame: ActionFacts.Frame(x: 345.5, y: 711, width: 1066, height: 135),
          role: "AXTextArea",
          name: "Message #general"
        )
    )
    #expect(clicked.task == "Post the 0.1.1 release note")

    let odd = try #require(facts[1])
    #expect(odd.element == ActionFacts.Element(frame: ActionFacts.Frame(x: 1, y: 2, width: 3, height: 4)))
    #expect(odd.task == nil, "an odd or empty task reads as none")

    #expect(facts[2]?.element == nil, "an empty frame marks nothing")
    #expect(facts[3]?.element == nil && facts[3]?.label == "Send", "an element with no frame reads as none")
    #expect(facts[4]?.element == nil && facts[4]?.task == nil)
  }

  @Test("a picture's facts as an earlier build wrote them, a window's title among them, still read in full")
  func earlierFactsStillRead() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "facts": {
          "action": "click", "outcome": "worked", "display": {"width": 1512, "height": 982},
          "point": {"x": 120, "y": 340}, "button": "left", "clicks": 1, "label": "Send",
          "element": {"frame": {"x": 100, "y": 330, "width": 40, "height": 20}, "role": "AXButton", "name": "Send"},
          "window": "Draft · Notes", "task": "Send the note", "at": 1792665102000, "endedAt": 1792665102400
        }}
      ]}
      """#.utf8
    )
    let facts = try #require(
      try ViewerClient.activity(body, response: reply(200))
        .entries
        .first?
        .facts
    )
    #expect(facts.action == .click)
    #expect(facts.point == ActionFacts.Point(x: 120, y: 340))
    #expect(facts.clicks == 1)
    #expect(facts.element?.name == "Send")
    #expect(facts.task == "Send the note")
    #expect(facts.at == Date(timeIntervalSince1970: 1_792_665_102))
  }

  @Test("an activity row's verb, app, label, bundle and keys of the wrong type or missing read as none")
  func namesReadLeniently() throws {
    let body = Data(
      #"""
      {"activity": [
        {"id": "1", "verb": 3, "app": "", "label": ["Save"], "bundleId": null, "keys": "⌘S"},
        {"id": "2", "keys": [["⌘", 7]]},
        {"id": "3"}
      ]}
      """#.utf8
    )
    let entries = try ViewerClient.activity(body, response: reply(200)).entries
    #expect(entries.count == 3)
    for entry in entries {
      #expect(entry.verb == nil)
      #expect(entry.app == nil)
      #expect(entry.label == nil)
      #expect(entry.bundleId == nil)
      #expect(entry.keys.isEmpty)
    }
  }
}
