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
    /// The alert.
    case orbitAlert = "orbit-alert"
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
  }
#endif
