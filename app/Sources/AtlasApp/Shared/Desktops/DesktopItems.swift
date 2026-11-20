import AtlasService
import AtlasStatus
import AtlasUI

/// What the windows say of the desktops while more than one is set up, decided once for the main window, the panel and
/// the menu bar: the items the picker lists, the title of the one shown, and the most urgent state of them all. The
/// desktop shown is said by the window's own mode, which knows its control and its picture; every other by its newest
/// status read (``DesktopDoing``). A VM waiting on its power or the person's maintenance says so, in blue while in
/// maintenance, whichever it is.
enum DesktopItems {
  /// The most urgent state of every desktop, as the menu bar shows it: its row, and its word, naming the desktop when
  /// it is not the one shown.
  struct Urgent: Equatable {
    /// The row, whose badge the glyph draws.
    var kind: Mode.Kind
    /// Its word, such as "Needs you on work".
    var word: String
  }

  /// The desktops the picker lists, each with its dot and word, the one shown checked; none while one desktop is set
  /// up, when nothing changes.
  ///
  /// - Parameters:
  ///   - list: The desktops, as last read.
  ///   - mode: The desktop shown's mode, as the window sees it.
  /// - Returns: The items, in the settings' order.
  static func items(_ list: DesktopList?, shown mode: Mode) -> [DesktopItem] {
    guard let list, list.desktops.count > 1 else { return [] }
    return list.desktops.map { entry in
      let isShown = isShown(entry, in: list)
      let doing = DesktopDoing.of(entry)
      if isShown, doing.kind != nil {
        return item(entry, word: mode.word, kind: mode.kind, isShown: true)
      }
      switch doing {
      case .mode(let kind):
        return item(entry, word: kind.word, kind: kind, isShown: isShown)
      case .standing(let standing):
        return DesktopItem(
          id: entry.id,
          title: Words.title(of: entry),
          word: Words.standing(standing) ?? "",
          tone: standing == .maintenance ? .link : .quiet,
          isViewed: isShown
        )
      }
    }
  }

  /// The title of the desktop shown, while more than one is set up.
  ///
  /// - Parameter list: The desktops, as last read.
  /// - Returns: Its title, or nil while one desktop is set up or none is named shown.
  static func shownTitle(_ list: DesktopList?) -> String? {
    shown(list)
      .map(Words.title(of:))
  }

  /// The desktop shown, while more than one is set up.
  ///
  /// - Parameter list: The desktops, as last read.
  /// - Returns: It, or nil while one desktop is set up or none is named shown.
  static func shown(_ list: DesktopList?) -> DesktopList.Entry? {
    guard let list, list.desktops.count > 1 else { return nil }
    return list.desktops.first { isShown($0, in: list) }
  }

  /// The most urgent state of every desktop: the shown one's mode beside every other's row, a VM waiting on its power
  /// or maintenance counting for nothing; the shown one's mode while one desktop is set up, or nothing else counts.
  ///
  /// - Parameters:
  ///   - list: The desktops, as last read.
  ///   - mode: The desktop shown's mode, as the window sees it.
  /// - Returns: The row and its word.
  static func mostUrgent(_ list: DesktopList?, shown mode: Mode) -> Urgent {
    let shown = Urgent(kind: mode.kind, word: mode.word)
    guard let list, list.desktops.count > 1, mode.kind != .connecting else { return shown }
    var others: [(kind: Mode.Kind, title: String)] = []
    var shownCounts = true
    for entry in list.desktops {
      let kind = DesktopDoing.of(entry).kind
      if isShown(entry, in: list) {
        shownCounts = kind != nil
      } else if let kind {
        others.append((kind, Words.title(of: entry)))
      }
    }
    let rivals = (shownCounts ? [mode.kind] : []) + others.map(\.kind)
    guard let most = Mode.Kind.mostUrgent(rivals) else { return shown }
    if shownCounts, most.urgency <= mode.kind.urgency { return shown }
    guard let other = others.first(where: { $0.kind == most }) else { return shown }
    return Urgent(kind: most, word: PanelWords.onDesktop(most.word, other.title))
  }

  /// Whether `entry` is the desktop shown: the one the list names, else the one marked viewed.
  ///
  /// - Parameters:
  ///   - entry: A desktop.
  ///   - list: Every desktop.
  /// - Returns: Whether it is shown.
  private static func isShown(_ entry: DesktopList.Entry, in list: DesktopList) -> Bool {
    list.viewed.map { $0 == entry.id } ?? entry.isViewed
  }

  /// An item in a row's word, tone and mark.
  ///
  /// - Parameters:
  ///   - entry: The desktop.
  ///   - word: Its word.
  ///   - kind: Its row.
  ///   - isShown: Whether it is the one shown.
  /// - Returns: The item.
  private static func item(_ entry: DesktopList.Entry, word: String, kind: Mode.Kind, isShown: Bool) -> DesktopItem {
    DesktopItem(
      id: entry.id,
      title: Words.title(of: entry),
      word: word,
      tone: kind.tone,
      mark: kind.mark,
      isViewed: isShown
    )
  }
}
