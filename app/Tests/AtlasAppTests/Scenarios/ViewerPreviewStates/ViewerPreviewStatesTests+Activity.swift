#if DEBUG
  import AtlasService
  import AtlasServiceTesting
  import Foundation
  import Testing

  @testable import AtlasApp
  import AtlasUI

  extension ViewerPreviewStatesTests {
    @Test(
      "Activity samples match after starting with different locales calendars clocks and time zones",
      arguments: ["en_US", "de_DE"],
      ["America/Los_Angeles", "Asia/Tokyo"]
    )
    func activitySamplesIgnoreInitialEnvironment(locale: String, timeZone: String) throws {
      var initial = ViewerSnapshot.presenterSample
      initial.locale = Locale(identifier: locale)
      initial.timeZone = try #require(TimeZone(identifier: timeZone))
      initial.calendar = Calendar(identifier: .buddhist)
      initial.calendar.locale = initial.locale
      initial.calendar.timeZone = initial.timeZone
      // Just after a different day's midnight, far from the sample's morning.
      initial.date = try #require(
        initial.calendar.date(from: DateComponents(year: 2569, month: 10, day: 30, second: 1))
      )
      let samples = Self(initialSnapshot: initial)
      let presented = try samples.activitySamples()
        .merging(samples.sessionsSamples()) { _, state in state }
      for preview in PreviewStates.viewer {
        guard case .activity = preview.value.pane else { continue }
        let state = try #require(presented[preview.name], "\(preview.name) has no facts")
        #expect(
          preview.value == Self.held(preview.value, to: state),
          "\(preview.name), starting in \(locale), \(timeZone)"
        )
      }
    }

    /// The Activity tab's samples' facts, by the sample's name: times on a German clock in UTC, for
    /// deterministic timestamps.
    ///
    /// - Returns: What the presenter shows for each.
    /// - Throws: A decoding error, which a fixed reply never gives.
    func activitySamples() throws -> [String: ViewerState] {
      let working = try reported("agent", state: "working", activity: Self.working)
      let idle = try reported("observing", state: "idle")
      let onLog = { (log: ActivityLog, snapshot: inout ViewerSnapshot) in
        Self.onPlace(.activity, at: Self.activityReadAt, &snapshot)
        snapshot.panes.activity.log = log
        snapshot.panes.activity.readAt = Self.activityReadAt
        snapshot.panes.shots.shots = Self.shots
        snapshot.panes.icons = Self.icons
        Self.keepThumbnails(&snapshot)
      }
      return [
        "Activity": shown(working) { onLog(Self.activityLog, &$0) },
        "Activity, nothing yet": shown(try reported("observing", state: "idle", activity: [])) { snapshot in
          Self.onPlace(.activity, at: Self.activityReadAt, &snapshot)
          snapshot.panes.activity.log = ActivityLog()
          snapshot.panes.activity.readAt = Self.activityReadAt
        },
        "Activity, paused": shown(idle) { snapshot in
          onLog(Self.activityLog, &snapshot)
          snapshot.session.paused = true
        },
        "Activity, a run opened": shown(working) { snapshot in
          onLog(Self.activityLog, &snapshot)
          snapshot.panes.activity.choices = [Self.textEditRun: true]
        },
        "Activity, problems": shown(working) { snapshot in
          onLog(Self.activityLog, &snapshot)
          snapshot.panes.activity.filter = .problems
        },
        "Activity, days before": shown(idle) { onLog(Self.daysLog, &$0) },
        "Activity, many sessions": shown(idle) { onLog(Self.manyLog, &$0) },
        "Activity, a run's pictures": shown(working) { snapshot in
          onLog(Self.activityLog, &snapshot)
          snapshot.panes.activity.pictureRuns[Self.opieRun] = false
        },
        "A picture": shown(working) { snapshot in
          onLog(Self.activityLog, &snapshot)
          snapshot.panes.activity.pictureRuns[Self.opieRun] = false
          snapshot.panes.activity.sheet = ViewerPictureFocus(
            run: Self.opieRun,
            item: .shot(PreviewStates.shotNames[2]),
            everyStep: false
          )
        },
      ]
    }

    /// The samples' captures, newest first, each 2560 × 1440.
    static let shots: [Screenshot] = zip(
      PreviewStates.shotNames,
      [1_792_665_105, 1_792_665_104, 1_792_665_080, 1_792_665_074, 1_792_663_923, 1_792_663_121]
    )
    .map { Screenshot(name: $0, takenAt: Date(timeIntervalSince1970: $1), width: 2560, height: 1440) }

    /// Every capture's thumbnail read, and every picture's taken before an action: the sample desktop at the
    /// captures' shape, known by the file's name.
    ///
    /// - Parameter snapshot: The facts to change.
    static func keepThumbnails(_ snapshot: inout ViewerSnapshot) {
      for name in PreviewStates.shotNames + PreviewStates.beforeNames {
        guard let picture = PreviewStates.shotPicture(name) else { continue }
        snapshot.panes.thumbnails.keep(picture, for: name)
      }
    }

    /// 10:31:42 on 22 October 2026, in UTC: when the Activity sample's log is read.
    static let activityReadAt = Date(timeIntervalSince1970: 1_792_665_102)
    /// The TextEdit run's identity in the Activity sample.
    static let textEditRun = "session:|e1"
    /// The Opie run's identity in the Activity sample, which took its screenshot.
    static let opieRun = "session:|o1"

    /// A row an agent wrote in an app, as the service lists it.
    ///
    /// - Parameters:
    ///   - id: The row's id.
    ///   - says: What it did, in the service's words.
    ///   - age: How long before the reading it started.
    ///   - verb: The verb Atlas wrote for it.
    ///   - app: The app's name and bundle identifier.
    ///   - outcome: How it went; worked by default.
    ///   - label: The control or field it named, or the keys; none by default.
    ///   - keys: The keys as caps; none by default.
    ///   - reason: Why it did not work; none by default.
    ///   - operation: The operation; input by default.
    ///   - session: The host session's key; Terminal's by default.
    ///   - before: The picture taken right before it; none by default.
    ///   - facts: What was recorded to mark its picture; none by default.
    /// - Returns: The row.
    static func row(
      _ id: String,
      _ says: String,
      age: Double,
      verb: String,
      app: (name: String, id: String?)?,
      outcome: ActivityLog.Outcome = .worked,
      label: String? = nil,
      keys: [[String]] = [],
      reason: String? = nil,
      operation: String = "input",
      session: String? = nil,
      before: String? = nil,
      facts: ActionFacts? = nil
    ) -> ActivityLog.Entry {
      ActivityLog.Entry(
        id: id,
        says: says,
        outcome: outcome,
        ageSeconds: age,
        reason: reason,
        operation: operation,
        session: session,
        capture: operation == "screenshot" ? PreviewStates.shotNames[2] : nil,
        verb: verb,
        app: app?.name,
        label: label,
        bundleId: app?.id,
        keys: keys,
        before: before,
        facts: facts
      )
    }

    /// The display the Activity sample's pictures show, in points: half the samples' 2560 by 1440 captures.
    static let sampleDisplay = ActionFacts.Size(width: 1280, height: 720)
    /// Where the sample's typing and ⌘S went in Opie: its Name field, at ``PreviewStates/sampleSpot``.
    static let sampleSpot = ActionFacts.Point(
      x: PreviewStates.sampleSpot.x * sampleDisplay.width,
      y: PreviewStates.sampleSpot.y * sampleDisplay.height
    )

    /// Opie, by its sample bundle identifier.
    static let opie = (name: "Opie", id: Optional(SampleAppIcons.opie))
    /// Notes.
    static let notes = (name: "Notes", id: Optional(SampleAppIcons.notes))
    /// TextEdit.
    static let textEdit = (name: "TextEdit", id: Optional(SampleAppIcons.textEdit))
    /// Safari.
    static let safari = (name: "Safari", id: Optional(SampleAppIcons.safari))
    /// A copy Atlas made, whose icon only the helper could read.
    static let buildTool = (name: "BuildTool", id: Optional<String>.none)

    /// The Activity sample's log, newest first, read at 10:31:42: an agent at work in Opie, its typing in Notes refused
    /// and then done, nine actions in TextEdit with a click that failed, and a command a while before.
    static let activityLog = ActivityLog(entries: [
      row(
        "o6",
        "clicking Show Center in Opie",
        age: 0,
        verb: "click",
        app: opie,
        outcome: .running,
        label: "Show Center"
      ),
      row("o5", "scrolled in Opie", age: 4, verb: "scroll", app: opie),
      row("o4", "took a screenshot", age: 22, verb: "screenshot", app: nil, operation: "screenshot"),
      row(
        "o3",
        "typed into Name in Opie",
        age: 25,
        verb: "type",
        app: opie,
        label: "Name",
        before: PreviewStates.beforeNames[0],
        facts: ActionFacts(
          action: .type,
          outcome: .worked,
          display: sampleDisplay,
          point: sampleSpot,
          text: PreviewStates.sampleTyped,
          length: PreviewStates.sampleTyped.count
        )
      ),
      row(
        "o2",
        "pressed ⌘S in Opie",
        age: 27,
        verb: "key",
        app: opie,
        label: "⌘S",
        keys: [["⌘", "S"]],
        before: PreviewStates.beforeNames[1],
        facts: ActionFacts(
          action: .key,
          outcome: .worked,
          display: sampleDisplay,
          point: sampleSpot,
          keys: [["⌘", "S"]]
        )
      ),
      row("o1", "opened Opie", age: 30, verb: "open", app: opie, operation: "open"),
      row("n3", "typed in Notes", age: 186, verb: "type", app: notes),
      row(
        "n2",
        "didn't type in Notes",
        age: 190,
        verb: "type",
        app: notes,
        outcome: .notSent,
        reason: "Another app was in front"
      ),
      row("n1", "opened Notes", age: 200, verb: "open", app: notes, operation: "open"),
      row("e9", "pressed ⌘S in TextEdit", age: 440, verb: "key", app: textEdit, label: "⌘S", keys: [["⌘", "S"]]),
      row("e8", "typed in TextEdit", age: 452, verb: "type", app: textEdit),
      row("e7", "clicked Bold in TextEdit", age: 470, verb: "click", app: textEdit, label: "Bold"),
      row("e6", "clicked Format in TextEdit", age: 488, verb: "click", app: textEdit, label: "Format"),
      row(
        "e5",
        "couldn't click Save in TextEdit",
        age: 500,
        verb: "click",
        app: textEdit,
        outcome: .failed,
        reason: "No element is named Save"
      ),
      row("e4", "scrolled in TextEdit", age: 540, verb: "scroll", app: textEdit),
      row("e3", "typed in TextEdit", age: 566, verb: "type", app: textEdit),
      row("e2", "clicked in TextEdit", age: 590, verb: "click", app: textEdit),
      row("e1", "opened TextEdit", age: 621, verb: "open", app: textEdit, operation: "open"),
      row("c1", "ran a command", age: 900, verb: "run", app: nil, operation: "run"),
    ])

    /// A log from the days before: a session in Safari late yesterday, and BuildTool, a copy, two days before.
    static let daysLog = ActivityLog(entries: [
      row("s3", "clicked Reload in Safari", age: 40_000, verb: "click", app: safari, label: "Reload"),
      row("s2", "typed into Address in Safari", age: 40_010, verb: "type", app: safari, label: "Address"),
      row("s1", "opened Safari", age: 40_030, verb: "open", app: safari, operation: "open"),
      row("b3", "took a screenshot", age: 160_000, verb: "screenshot", app: nil, operation: "screenshot"),
      row("b2", "clicked Build in BuildTool", age: 160_020, verb: "click", app: buildTool, label: "Build"),
      row("b1", "opened BuildTool", age: 160_050, verb: "open", app: buildTool, operation: "open"),
    ])

    /// A morning of an agent moving between apps every few minutes, read at 10:31:42.
    static let manyLog = ActivityLog(
      entries: [opie, notes, textEdit, safari, buildTool, opie, notes, textEdit, safari, opie]
        .enumerated()
        .flatMap { index, app -> [ActivityLog.Entry] in
          let start = Double(60 + index * 300)
          return [
            row("m\(index)b", "clicked in \(app.name)", age: start, verb: "click", app: app),
            row("m\(index)a", "opened \(app.name)", age: start + 20, verb: "open", app: app, operation: "open"),
          ]
        }
    )
  }
#endif
