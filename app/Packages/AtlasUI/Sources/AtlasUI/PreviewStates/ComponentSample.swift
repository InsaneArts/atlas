#if DEBUG
  /// Each shared component's samples, by name, so the app's design renders draw them beside the windows.
  public enum ComponentSample: String, CaseIterable, Sendable {
    /// The capsule tabs.
    case capsuleTabs = "capsule-tabs"
    /// Every capsule button look, the alert's included.
    case capsuleButtons = "capsule-buttons"
    /// The frosted captions over a picture.
    case captionStack = "caption-stack"
    /// The status dot's marks.
    case statusDots = "status-dots"
    /// The Orbit, a status's mark, word and line.
    case statusLabel = "status-label"
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
    /// The icon tabs.
    case iconTabs = "icon-tabs"
    /// A settings form's rows.
    case formRow = "form-row"
    /// The popup button.
    case popupButton = "popup-button"
    /// The app icon at every tile v3 names (B1).
    case appIconTiles = "app-icon-tiles"
    /// The account picture in its circle at each size (B1).
    case accountPictureSizes = "account-picture-sizes"
    /// Choose's accounts in a grouped list, the new one picked (D6, D7).
    case listRowChoices = "list-row-choices"
    /// A list row with each lead.
    case listRowLeads = "list-row-leads"
    /// A list row with each trailing kind.
    case listRowTrailing = "list-row-trailing"
    /// A list row at each height, in its group.
    case listRowSizes = "list-row-sizes"
    /// A grouped list scrolling past three and a half rows (D10).
    case groupedListScrolling = "grouped-list-scrolling"
    /// The status line, working, a problem and done (D4).
    case statusLineKinds = "status-line-kinds"
    /// The progress track at its first, a middle and its last step (D8).
    case progressTrackSteps = "progress-track-steps"
    /// The heroes, success and problem (D5).
    case heroDiscKinds = "hero-disc-kinds"
    /// The chips on the picture (V5, V6).
    case glassChipKinds = "glass-chip-kinds"
    /// The round glass buttons, by the capsule style (U-8).
    case capsuleButtonCircles = "capsule-button-circles"
    /// The viewer's actions, Keys and Stop in their slots (D11).
    case actionSlotActions = "action-slot-actions"
    /// The symbol tile at each size (U-7).
    case symbolTileSizes = "symbol-tile-sizes"
    /// Every symbol Atlas draws (D13).
    case symbolNames = "symbol-names"
  }
#endif
