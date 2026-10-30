#if DEBUG
  extension PreviewStates {
    /// The helper line's tooltip: every fact about the helper, one a line, as the window lists them.
    ///
    /// - Parameters:
    ///   - state: Whether input is ready, such as "Ready".
    ///   - display: The helper's display; 1440 by 900 by default.
    ///   - ready: Whether it is ready to act, one check a line; every check passing by default.
    ///   - picture: How often the picture updates; Live by default.
    ///   - newest: How old the newest picture is; just now by default.
    ///   - login: When the helper logged in and the agents connected, once read; none by default.
    /// - Returns: The tooltip.
    static func viewerHelperHelp(
      state: String = "Ready",
      display: String = "1440 × 900",
      ready: [String] = ["Ready to act: All checks pass"],
      picture: String = "Live",
      newest: String = "just now",
      login: [String] = []
    ) -> String {
      (["Account: Atlas", "State: \(state)", "Display: \(display)"] + ready
        + ["Picture: \(picture)", "Newest picture: \(newest)"] + login)
        .joined(separator: "\n")
    }

    /// What a click on the helper line offers, as the Helper menu does.
    ///
    /// - Parameters:
    ///   - recovers: Whether Recover… would help.
    ///   - logsOut: Whether the helper's state lets it log out; idle, in use elsewhere and stopped do.
    ///   - alertUp: Whether an alert is up, which holds Recover…, Switch… and Log Out….
    ///   - name: The helper's name, once known.
    /// - Returns: The menu's sections.
    static func viewerHelperMenu(
      recovers: Bool = false,
      logsOut: Bool = false,
      alertUp: Bool = false,
      name: String? = "Atlas"
    ) -> [ViewerMenuSection] {
      [
        ViewerMenuSection([
          ViewerMenuItem("Check Again", command: .checkAgain, shortcut: ViewerMenuShortcut("r")),
          ViewerMenuItem("Open Settings on Helper…", command: .openSettings),
        ]),
        ViewerMenuSection([ViewerMenuItem("Recover…", command: .askRecover, isEnabled: recovers && !alertUp)]),
        ViewerMenuSection([
          ViewerMenuItem(
            name.map { "Switch to “\($0)”…" } ?? "Switch to the Helper…",
            command: .askSwitch,
            isEnabled: !alertUp
          ),
          ViewerMenuItem(
            name.map { "Log Out “\($0)”…" } ?? "Log Out the Helper…",
            command: .askLogOut,
            isEnabled: logsOut && !alertUp
          ),
        ]),
      ]
    }

    /// The helper line while it can act.
    ///
    /// - Parameters:
    ///   - logsOut: Whether the helper's state lets it log out.
    ///   - alertUp: Whether an alert is up.
    ///   - picture: How often the picture updates.
    ///   - state: Whether input is ready.
    /// - Returns: The line.
    static func viewerHelperReady(
      logsOut: Bool,
      alertUp: Bool = false,
      picture: String = "Live",
      state: String = "Ready"
    ) -> ViewerHelperLine {
      ViewerHelperLine(
        tone: .ready,
        words: "Helper ready · atlas",
        help: viewerHelperHelp(state: state, picture: picture),
        menu: viewerHelperMenu(logsOut: logsOut, alertUp: alertUp)
      )
    }

    /// Send Key's submenu in a session's menu: the Mac's keys and an app's, which go to the helper only while this
    /// window holds control.
    ///
    /// - Parameter inControl: Whether this window holds control.
    /// - Returns: The item.
    static func viewerSendKeys(inControl: Bool) -> ViewerMenuItem {
      let mac = ["Switch Apps (⌘⇥)", "Spotlight (⌘Space)", "Mission Control (⌃↑)"]
      let app = ["Close Window (⌘W)", "Quit App (⌘Q)", "Escape (⎋)"]
      let items = { (titles: [String], start: Int) in
        titles.enumerated()
          .map { offset, title in
            ViewerMenuItem(title, command: .sendKey(index: start + offset), isEnabled: inControl)
          }
      }
      return ViewerMenuItem(
        "Send Key",
        isEnabled: inControl,
        submenu: [
          ViewerMenuSection(items(mac, .zero), title: "Mac Shortcuts"),
          ViewerMenuSection(items(app, mac.count), title: "App Shortcuts"),
        ]
      )
    }

    /// What a right click on a session's row offers.
    ///
    /// - Parameters:
    ///   - key: The session's key.
    ///   - facts: Every fact about it, one a line.
    ///   - inControl: Whether this window holds control, so Send Key can be chosen.
    ///   - recover: Whether the session needs the person, so Recover… is offered, and whether it can be chosen.
    ///   - cleanup: Whether Clean Up This Session's Work… is offered, and whether it can be chosen.
    /// - Returns: The menu's sections.
    static func viewerSessionMenu(
      _ key: String,
      facts: [String],
      inControl: Bool = false,
      recover: Bool? = nil,
      cleanup: Bool? = nil
    ) -> [ViewerMenuSection] {
      var sections = [
        ViewerMenuSection([
          ViewerMenuItem("Show Activity", command: .showActivity(session: key)),
          ViewerMenuItem("Show Screenshots", command: .showScreenshots(session: key)),
        ])
      ]
      if let recover {
        sections.append(ViewerMenuSection([ViewerMenuItem("Recover…", command: .askRecover, isEnabled: recover)]))
      }
      sections.append(ViewerMenuSection([viewerSendKeys(inControl: inControl)]))
      if let cleanup {
        sections.append(
          ViewerMenuSection([
            ViewerMenuItem(
              "Clean Up This Session's Work…",
              command: .askCleanup(session: key),
              isEnabled: cleanup,
              role: .destructive
            )
          ])
        )
      }
      sections.append(ViewerMenuSection(facts.map(ViewerMenuItem.info), title: "Details"))
      return sections
    }

    /// A session's tooltip: its name and line, then its facts.
    ///
    /// - Parameters:
    ///   - name: Its name.
    ///   - line: What it last did.
    ///   - facts: Every fact about it, one a line.
    ///   - receipt: What its last cleanup did, one step a line; none by default.
    /// - Returns: The tooltip.
    static func viewerSessionHelp(_ name: String, _ line: String, facts: [String], receipt: [String] = []) -> String {
      ([name, line] + facts + (receipt.isEmpty ? [] : ["Last cleanup"] + receipt))
        .joined(separator: "\n")
    }

    /// Claude Code's menu while it has the helper, for the menu's own preview.
    static let viewerSessionMenu = viewerSessionMenu(
      SessionSamples.claudeKey,
      facts: ["Host: Claude Code", "Folder: understudy", "Started: 09:12", "Last seen: just now"],
      cleanup: true
    )
  }
#endif
