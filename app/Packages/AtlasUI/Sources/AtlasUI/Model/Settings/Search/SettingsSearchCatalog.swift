/// Settings' fixed presentation words and the local metadata it has already loaded.
public enum SettingsSearchCatalog {
  /// The startup control and its section.
  public static let startup = SettingsSearchEntry(
    id: .startup,
    title: "Open Atlas at login",
    detail: "Atlas sits in the menu bar from the start",
    section: "Startup",
    sectionDetail: "Keep Atlas available from the menu bar.",
    aliases: ["startup", "login"]
  )
  /// The screen control and its exact options.
  public static let cadence = SettingsSearchEntry(
    id: .cadence,
    title: "Screen updates",
    detail: "How often Atlas's windows show the helper's screen",
    aliases: ["Live", "Every 5 Seconds", "Paused", "pause"]
  )
  /// The notification control and its section.
  public static let notifications = SettingsSearchEntry(
    id: .notifications,
    title: "Notify me",
    detail: "When an action may not have finished, or the helper logs out",
    section: "When Atlas needs you"
  )
  /// The helper's account and Switch control.
  public static let account = SettingsSearchEntry(
    id: .account,
    title: "Helper account",
    detail: "The hidden helper login",
    aliases: ["Switch"]
  )
  /// The readiness section, also discoverable before the helper reports checks.
  public static let readiness = SettingsSearchEntry(
    id: .readiness,
    title: "Ready to act",
    detail: "Check the helper and the permissions it needs.",
    aliases: [
      "Check Again", "permission", "accessibility", "screen recording", "native dialogs", "doctor", "diagnostics",
    ]
  )
  /// Recovery is a navigation target, never a search command.
  public static let recover = SettingsSearchEntry(
    id: .recover,
    title: "Recover the helper",
    detail: "Restarts its mouse and keyboard",
    section: "When something goes wrong",
    sectionDetail: "Recover input or end the helper's login.",
    aliases: ["Recover…", "Restart"]
  )
  /// Logout is a navigation target, never a search command.
  public static let logOut = SettingsSearchEntry(
    id: .logOut,
    title: "Log out the helper",
    detail: "Quits its apps until you log in again",
    section: recover.section,
    sectionDetail: recover.sectionDetail,
    aliases: ["Log Out…"]
  )
  /// The existing diagnostics route.
  public static let support = SettingsSearchEntry(
    id: .support,
    title: "Copy Diagnostics",
    section: "Support",
    sectionDetail: "Copy Atlas's diagnostic report for support."
  )
  /// Atlas's identity and product explanation.
  public static let identity = SettingsSearchEntry(
    id: .identity,
    title: "Atlas",
    detail: "A hidden second desktop where agents test your apps while you work."
  )
  /// The existing update controls.
  public static let updates = SettingsSearchEntry(
    id: .updates,
    title: "Check for Updates…",
    section: "Updates",
    sectionDetail: "Check for a newer Atlas release",
    aliases: ["update", "version", "install"]
  )
  /// The existing setup route.
  public static let setup = SettingsSearchEntry(
    id: .setup,
    title: "Getting Started…",
    section: "Setup",
    sectionDetail: "Open Atlas's setup window."
  )
  /// General's navigation group.
  public static let everydayTitle = "Everyday preferences"
  /// General's navigation group explanation.
  public static let everydayDetail = "Screen updates and when Atlas asks for you"
  /// The agents section title.
  public static let agentsTitle = "Coding agents"
  /// The agents section explanation.
  public static let agentsDetail = "Let supported agents use the helper."
  /// The project disclosure's section title.
  public static let projectsTitle = "Project instructions"
  /// The project disclosure's explanation.
  public static let projectsDetail = "Projects taught to agents without the Atlas plugin."

  /// Builds metadata only from the presentation and already loaded rows; no file or service reads.
  ///
  /// - Parameter state: The current presentation.
  /// - Returns: Stable page order, followed by rows in presentation order.
  public static func entries(_ state: SettingsState) -> [SettingsSearchEntry] {
    let pages = SettingsDestination.pages
      .map { destination in
        SettingsSearchEntry(
          id: .page(destination),
          title: destination.title,
          detail: destination.description,
          aliases: destination == .root(.general)
            ? [everydayTitle, everydayDetail]
            : destination == .projects ? [projectsTitle, projectsDetail] : []
        )
      }
    let agents = state.agents.agents.map { agent in
      SettingsSearchEntry(
        id: .agent(agent.id),
        title: agent.title,
        detail: agent.detail,
        section: agentsTitle,
        sectionDetail: agentsDetail,
        aliases: ["Connect", "Disconnect", "Show File"] + (agent.id == "claude" ? ["Claude Code", "Claude app"] : [])
      )
    }
    let projects = state.agents.projects.map { project in
      SettingsSearchEntry(
        id: .project(project.id),
        title: project.name,
        detail: project.detail,
        aliases: [project.id, project.forgetLabel, "Forget"]
      )
    }
    let checks = state.helper.ready.map { row in
      SettingsSearchEntry(
        id: .check(row.searchID),
        title: row.item.name,
        detail: row.item.text,
        section: readiness.title
      )
    }
    let cleanup = state.helper.closeLogin.map { row in
      SettingsSearchEntry(id: .cleanup, title: row.title, detail: row.detail, aliases: [row.button])
    }
    let helper = SettingsSearchEntry(
      id: .account,
      title: state.helper.account.name,
      detail: state.helper.account.line,
      aliases: [account.title] + account.aliases
    )
    return pages + [helper] + (cleanup.map { [$0] } ?? []) + [readiness] + checks
      + [recover, logOut, startup, cadence, notifications] + agents + projects + [
        support,
        SettingsSearchEntry(
          id: .identity,
          title: identity.title,
          detail: "\(state.about.version) · \(identity.detail)",
          aliases: ["version"]
        ), updates, setup,
      ]
  }
}
