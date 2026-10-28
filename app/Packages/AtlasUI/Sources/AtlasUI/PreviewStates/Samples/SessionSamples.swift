#if DEBUG
  /// The host sessions every sample draws: Claude Code · understudy acting, Codex · atlas-landing waiting 8 s, Cursor
  /// · opie idle for 4 min, a Terminal one, Pi · notes-sync idle for 20 min, and the variant where Claude Code's click
  /// may not have finished.
  enum SessionSamples {
    /// Claude Code's session key.
    static let claudeKey = "3f9a1c07b2e4"
    /// Codex's session key.
    static let codexKey = "8be20c4d1a77"
    /// Cursor's session key.
    static let cursorKey = "5d21e9a04c3f"
    /// Pi's session key.
    static let piKey = "c0ffee5e5510"
    /// Terminal's key: the commands no host named.
    static let terminalKey = ""

    /// Claude Code's session name.
    static let claudeName = "Claude Code · understudy"
    /// Codex's session name.
    static let codexName = "Codex · atlas-landing"
    /// Cursor's session name.
    static let cursorName = "Cursor · opie"
    /// Pi's session name.
    static let piName = "Pi · notes-sync"
    /// Terminal's name.
    static let terminalName = "Terminal"

    /// Claude Code's logo.
    static let claudeIcon = RowIcon.app(AgentLogos.claude)
    /// Codex's logo.
    static let codexIcon = RowIcon.app(AgentLogos.codex)
    /// Cursor's logo.
    static let cursorIcon = RowIcon.app(AgentLogos.cursor)
    /// Pi's mark.
    static let piIcon = RowIcon.mark(.pi)
    /// Terminal's tile.
    static let terminalIcon = RowIcon.tile(.text(">_"))
    /// Every host's icon, for the icon's samples.
    static let icons = [claudeIcon, codexIcon, cursorIcon, piIcon, RowIcon.mark(.opencode), terminalIcon]

    /// What Claude Code is doing.
    static let claudeDoing = "Clicking Show Center in Opie"

    /// Claude Code with the helper.
    static let claudeActing = PanelSessionEntry(
      id: claudeKey,
      icon: claudeIcon,
      name: claudeName,
      line: claudeDoing,
      state: "Acting",
      tone: .active,
      label: "Claude Code · understudy, Acting, Clicking Show Center in Opie"
    )

    /// Claude Code's click may not have finished.
    static let claudeNeedsYou = PanelSessionEntry(
      id: claudeKey,
      icon: claudeIcon,
      name: claudeName,
      line: "A click may not have finished",
      isUrgent: true,
      state: "Needs you",
      tone: .attention,
      label: "Claude Code · understudy, Needs you, A click may not have finished"
    )

    /// Codex waiting its turn while Claude Code has the helper.
    static let codexWaiting = PanelSessionEntry(
      id: codexKey,
      icon: codexIcon,
      name: codexName,
      line: "Waiting for the helper · 8 s",
      state: "Waiting",
      tone: .quiet,
      mark: .ring,
      label: "Codex · atlas-landing, Waiting, Waiting for the helper · 8 s"
    )

    /// Codex waiting until the person recovers Claude Code's click.
    static let codexRecovery = PanelSessionEntry(
      id: codexKey,
      icon: codexIcon,
      name: codexName,
      line: "Waits until you recover",
      state: "Waiting",
      tone: .quiet,
      mark: .ring,
      label: "Codex · atlas-landing, Waiting, Waits until you recover"
    )

    /// Cursor, idle since it opened Opie.
    static let cursorIdle = PanelSessionEntry(
      id: cursorKey,
      icon: cursorIcon,
      name: cursorName,
      line: "Last: opened Opie · 4 min ago",
      state: "Idle",
      tone: .quiet,
      label: "Cursor · opie, Idle, Last: opened Opie · 4 min ago"
    )

    /// Pi, idle since it opened Notes, before it ended.
    static let piIdle = PanelSessionEntry(
      id: piKey,
      icon: piIcon,
      name: piName,
      line: "Last: opened Notes · 20 min ago",
      state: "Idle",
      tone: .quiet,
      label: "Pi · notes-sync, Idle, Last: opened Notes · 20 min ago"
    )

    /// Terminal, idle since a command took a screenshot.
    static let terminalIdle = PanelSessionEntry(
      id: terminalKey,
      icon: terminalIcon,
      name: terminalName,
      line: "Last: took a screenshot · 1 min ago",
      state: "Idle",
      tone: .quiet,
      label: "Terminal, Idle, Last: took a screenshot · 1 min ago"
    )

    /// Claude Code's session in each state its row takes, for the row's Interactive preview.
    static let claudeStates = [
      claudeActing,
      claudeNeedsYou,
      PanelSessionEntry(
        id: claudeKey,
        icon: claudeIcon,
        name: claudeName,
        line: "Waiting for the helper · 12 s",
        state: "Waiting",
        tone: .quiet,
        mark: .ring,
        label: "Claude Code · understudy, Waiting, Waiting for the helper · 12 s"
      ),
      PanelSessionEntry(
        id: claudeKey,
        icon: claudeIcon,
        name: claudeName,
        line: "Last: clicked Show Center in Opie · 1 min ago",
        state: "Idle",
        tone: .quiet,
        label: "Claude Code · understudy, Idle, Last: clicked Show Center in Opie · 1 min ago"
      ),
    ]

    /// Every live session the samples list, in the service's order.
    static let panelEntries = [claudeActing, codexWaiting, cursorIdle, terminalIdle, piIdle]
  }
#endif
