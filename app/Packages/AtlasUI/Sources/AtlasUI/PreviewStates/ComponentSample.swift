#if DEBUG
  /// Each shared component's samples, by name, so the app's design renders draw them beside the windows.
  public enum ComponentSample: String, CaseIterable, Sendable {
    /// Every capsule button look, the alert's included.
    case capsuleButtons = "capsule-buttons"
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
    /// The search field.
    case searchField = "search-field"
    /// A group's label.
    case groupLabel = "group-label"
    /// The popup button.
    case popupButton = "popup-button"
    /// The app icon at every supported tile size.
    case appIconTiles = "app-icon-tiles"
    /// Apps' icons and the tiles drawn for apps without one, side by side at a row's and a card's size.
    case appGlyph = "app-glyph"
    /// The drawn tile in every tint, at a card's, a row's and a chip's size.
    case monogramTints = "monogram-tints"
    /// A tag in each tone.
    case tagTones = "tag-tones"
    /// What leads an action: an app, an app's tile, Atlas, the person and commands, at a moment's and a Recent row's
    /// size.
    case activityGlyphs = "activity-glyphs"
    /// Soft keycaps alone and in chords.
    case keycapsSoft = "keycaps-soft"
    /// The Apps sample's cards in a grid, the front one raised, with their pictures, maps and tags.
    case appCards = "app-cards"
    /// Activity's steps of every kind and mark.
    case activitySteps = "activity-steps"
    /// Activity's moments, open, folded and held.
    case activityMoments = "activity-moments"
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
    case pictureChipKinds = "picture-chip-kinds"
    /// The round buttons, by the capsule style.
    case capsuleButtonCircles = "capsule-button-circles"
    /// The viewer's actions, Keys and Stop in their slots.
    case actionSlotActions = "action-slot-actions"
    /// The symbol tile at each size.
    case symbolTileSizes = "symbol-tile-sizes"
    /// Every symbol Atlas draws.
    case symbolNames = "symbol-names"
    /// The tabs in a track by word and by symbol, and as symbols over words.
    case tabs
    /// Settings' sections: a name, rows of each kind, small print and the status lines under a group.
    case groupedSection = "grouped-section"
    /// The status pill in each tone.
    case statusPill = "status-pill"
    /// The latest-action line, as each of three steps draws it.
    case lineSwapLines = "line-swap-lines"
    /// A session icon's badge, acting, waiting, needing the person and none.
    case stateBadgeStates = "state-badge-states"
  }
#endif
