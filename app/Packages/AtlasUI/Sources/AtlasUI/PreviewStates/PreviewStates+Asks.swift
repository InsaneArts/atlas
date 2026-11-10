#if DEBUG
  import CoreGraphics

  extension PreviewStates {
    /// The release note the sample agent would send, as its question shows it.
    static let askMessage = "Release 0.1.1 is out: faster setup and fixes for typing in the helper."

    /// The sample chat app's send button, as shares of its picture: 1270, 710, 57 by 59 on a 1440 by 900 display.
    static let askSendButton = CGRect(x: 1270.0 / 1440, y: 710.0 / 900, width: 57.0 / 1440, height: 59.0 / 900)

    /// The sample chat app's #general row, as shares of its picture: 91, 264, 246 by 44 on a 1440 by 900 display.
    static let askGeneralRow = CGRect(x: 91.0 / 1440, y: 264.0 / 900, width: 246.0 / 1440, height: 44.0 / 900)

    /// A question from Claude Code, 42 seconds old.
    ///
    /// - Parameters:
    ///   - id: The question's id.
    ///   - title: What it asks about.
    ///   - body: What would be sent; none by default.
    ///   - choices: The answers offered; all three by default.
    ///   - account: The account Allow for This Task covers; Slack by default.
    /// - Returns: The question.
    static func ask(
      _ id: String,
      _ title: String,
      body: AskCardBody? = nil,
      choices: [AskChoice] = AskChoice.allCases,
      account: String = "Slack"
    ) -> AskCardState {
      AskCardState(
        id: id,
        icon: .app(AgentLogos.claude),
        who: "Claude Code · release",
        waited: "0:42",
        title: title,
        task: "Task: post the 0.1.1 release notes",
        body: body,
        choices: choices,
        scope: choices.contains(.task)
          ? "For this task covers Claude Code in \(account) until it finishes or for 1 hour." : nil,
        label: "Claude Code · release asks: \(title)"
      )
    }

    /// A question about sending a typed message in Slack, the message shown.
    static let askTyped = ask("a1", "Send a message in Slack", body: .message(askMessage))

    /// A question about clicking Send in Slack, its picture ringed.
    @MainActor static let askNamedClick = ask(
      "a2",
      "Click “Send” in Slack",
      body: SampleDesktop.chat("ask-a2", stage: .typed)
        .flatMap { AskCardBody.ringed($0, element: askSendButton) }
    )

    /// A question about clicking a button with no name in Slack, its picture ringed.
    @MainActor static let askUnnamedClick = ask(
      "a3",
      "Click a button with no name in Slack",
      body: SampleDesktop.chat("ask-a3", stage: .typed)
        .flatMap { AskCardBody.ringed($0, element: askGeneralRow) }
    )

    /// A question about typing into a password field on github.com: only how many characters.
    static let askPassword = ask(
      "a4",
      "Type in a password field in github.com",
      body: .note("12 characters in a password field"),
      account: "github.com"
    )

    /// A question that offers Allow for This Task and Deny alone, as one whose message couldn't be bound.
    static let askTaskOnly = ask("a5", "Send a message in Slack", body: .message(askMessage), choices: [.task])

    /// What the person allowed for the task in GitHub, 52 minutes left.
    static let askAllowed = AllowedRowState(
      id: "k1\u{1F}site\u{1F}github.com\u{1F}",
      title: "Allowed in GitHub for this task",
      detail: "Claude Code · 52 min left",
      endLabel: "End what you allowed in GitHub"
    )
  }
#endif
