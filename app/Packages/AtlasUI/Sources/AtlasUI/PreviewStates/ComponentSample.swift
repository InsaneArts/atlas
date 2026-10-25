#if DEBUG
  /// Each shared component's samples, by name, so the app's design renders draw them beside the windows.
  public enum ComponentSample: String, CaseIterable, Sendable {
    /// Every capsule button look, the alert's included.
    case capsuleButtons = "capsule-buttons"
    /// The frosted captions over a picture.
    case captionStack = "caption-stack"
    /// The status dot's marks.
    case statusDots = "status-dots"
    /// A chip floating over a picture.
    case floatingChip = "floating-chip"
    /// The alert card, side by side and stacked, asked, running, refused and done.
    case alertCard = "alert-card"
    /// A message in place of a picture.
    case emptyMessage = "empty-message"
    /// One letterboxed picture in each look.
    case pictureFrame = "picture-frame"
    /// The live corner picture.
    case miniScreen = "mini-screen"
    /// A pane's title, count and tools.
    case paneHeader = "pane-header"
    /// The search field.
    case searchField = "search-field"
    /// A group's label.
    case groupLabel = "group-label"
    /// The popup button.
    case popupButton = "popup-button"
    /// The app icon at every supported tile size.
    case appIconTiles = "app-icon-tiles"
    /// The account picture in its circle at each size.
    case accountPictureSizes = "account-picture-sizes"
    /// Choose's accounts in a grouped list, the new one picked.
    case listRowChoices = "list-row-choices"
    /// A list row with each lead.
    case listRowLeads = "list-row-leads"
    /// A list row with each trailing kind.
    case listRowTrailing = "list-row-trailing"
    /// A list row at each height, in its group.
    case listRowSizes = "list-row-sizes"
    /// A grouped list scrolling past three and a half rows.
    case groupedListScrolling = "grouped-list-scrolling"
    /// The status line, working, a problem and done.
    case statusLineKinds = "status-line-kinds"
    /// The progress track at its first, a middle and its last step.
    case progressTrackSteps = "progress-track-steps"
    /// The heroes, success and problem.
    case heroDiscKinds = "hero-disc-kinds"
    /// The chips on the picture.
    case glassChipKinds = "glass-chip-kinds"
    /// The round glass buttons, by the capsule style.
    case capsuleButtonCircles = "capsule-button-circles"
    /// The viewer's actions, Keys and Stop in their slots.
    case actionSlotActions = "action-slot-actions"
    /// The symbol tile at each size.
    case symbolTileSizes = "symbol-tile-sizes"
    /// Every symbol Atlas draws.
    case symbolNames = "symbol-names"
    /// The tabs in a track by word and by symbol, and as symbols over words.
    case tabs
    /// An inspector's facts and checks.
    case facts
    /// Settings' sections: a name, rows of each kind, small print and the status lines under a group.
    case groupedSection = "grouped-section"
    /// The status pill in each tone.
    case statusPill = "status-pill"
  }
#endif
