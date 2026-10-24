import AppKit
import SwiftUI
import Testing

@testable import AtlasUI

@Suite("SettingsLayout")
struct SettingsLayoutTests {
  /// How large a view draws, settled, as a render draws it.
  ///
  /// - Parameter view: The view.
  /// - Returns: Its size in points.
  /// - Throws: When nothing was drawn.
  @MainActor
  private static func size(_ view: some View) throws -> CGSize {
    let renderer = ImageRenderer(
      content: view.environment(\.animatesArrivals, false)
        .environment(\.colorScheme, .dark)
    )
    renderer.scale = 1
    let image = try #require(renderer.cgImage, "nothing was drawn")
    return CGSize(width: image.width, height: image.height)
  }

  @Test("each Settings pane keeps its own scroll position when another pane is selected")
  @MainActor
  func eachPaneKeepsItsScrollPosition() async throws {
    var state = PreviewStates.settingsHelper
    let host = NSHostingView(rootView: SettingsPane(state: state, edit: { _ in }, send: { _ in }))
    let window = NSWindow(
      contentRect: CGRect(
        origin: .zero,
        size: CGSize(width: SettingsLayout.contentMaximumWidth, height: SettingsLayout.headerHeight)
      ),
      styleMask: [.titled],
      backing: .buffered,
      defer: false
    )
    window.contentView = host
    for _ in 0..<20 { await Task.yield() }
    host.layoutSubtreeIfNeeded()
    let scrolls = Self.scrollViews(in: host)
    try #require(scrolls.count == SettingsTab.allCases.count)
    for (index, scroll) in scrolls.enumerated() {
      let document = try #require(scroll.documentView)
      let overflow = document.bounds.height - scroll.contentView.bounds.height
      try #require(overflow > 0, "each pane must actually overflow")
      let offset = overflow * CGFloat(index + 1) / CGFloat(scrolls.count + 1)
      scroll.contentView.scroll(to: CGPoint(x: 0, y: offset))
      scroll.reflectScrolledClipView(scroll.contentView)
      #expect(scroll.contentView.bounds.minY > 0, "the pane must actually scroll before switching away")
    }
    let positions = scrolls.map { $0.contentView.bounds.origin }
    for tab in SettingsTab.allCases {
      state.tabs.selected = tab
      host.rootView = SettingsPane(state: state, edit: { _ in }, send: { _ in })
      for _ in 0..<20 { await Task.yield() }
      host.layoutSubtreeIfNeeded()
      #expect(Self.scrollViews(in: host) == scrolls, "switching panes retains their scrolling views")
      for (scroll, position) in zip(scrolls, positions) {
        #expect(scroll.contentView.bounds.origin == position, "\(tab) reset another pane's position")
      }
    }
  }

  /// The native scrolling views under a hosted Settings pane.
  ///
  /// - Parameter view: The hosting view or one of its descendants.
  /// - Returns: Its scrolling descendants in tree order.
  @MainActor
  private static func scrollViews(in view: NSView) -> [NSScrollView] {
    view.subviews.flatMap { child in
      (child as? NSScrollView)
        .map { [$0] } ?? scrollViews(in: child)
    }
  }

  @Test("Helper accounts keep one row when it fits and stack at minimum width or with long names")
  @MainActor
  func narrowAccountsStackAndWrap() throws {
    let helper = PreviewStates.settingsHelper.helper
    var long = helper.account
    long.name = Array(repeating: PreviewStates.settingsLongHelper.helper.account.name, count: 3)
      .joined(separator: " ")
    for width in [
      SettingsLayout.contentMaximumWidth,
      SettingsLayout.minimumWidth - SettingsLayout.sidebarWidth - SettingsLayout.pageSideInset * 2,
    ] {
      let shortSize = try Self.size(
        HelperAccountRow(account: helper.account, buttons: helper.buttons, send: { _ in })
          .frame(width: width)
      )
      let longSize = try Self.size(
        HelperAccountRow(account: long, buttons: helper.buttons, send: { _ in })
          .frame(width: width)
      )
      let oneRowHeight = SettingsLayout.accountPicture + Spacing.m * 2
      if width == SettingsLayout.contentMaximumWidth {
        #expect(shortSize.height == oneRowHeight, "the default window fits one row")
      } else {
        #expect(shortSize.height > oneRowHeight, "minimum-width actions occupy a second row")
      }
      #expect(longSize.height > shortSize.height, "the name wraps instead of truncating")
    }
  }

  @Test("ordinary sidebar rows occupy 32 points while Helper keeps its 52-point two-line slot")
  @MainActor
  func sidebarRowsKeepTheirRhythm() throws {
    let state = PreviewStates.settingsHelper
    for item in state.tabs.tabs {
      let helper = item.id == .helper
      let size = try Self.size(
        SettingsSidebarRow(item: item, subtitle: helper ? state.helper.account.status.label : nil, selected: false) {}
          .frame(width: SettingsLayout.sidebarWidth)
      )
      #expect(size.height == (helper ? 52 : 32))
    }
  }

  @Test("Check Again's results land top first, each 50 milliseconds after the row above")
  func checkAgainStaggersEachResult() {
    #expect(SettingsLayout.checkStagger(index: 0) == 0, "the first result lands at once")
    for index in 1..<4 {
      let gap = SettingsLayout.checkStagger(index: index) - SettingsLayout.checkStagger(index: index - 1)
      #expect(abs(gap - 0.05) < 1e-9, "row \(index)")
    }
  }

  @Test("About keeps one height whatever its update line says, so the window never grows as a check goes")
  @MainActor
  func aboutKeepsOneHeight() throws {
    let statuses: [SettingsUpdateStatus?] = [
      nil,
      .working("Checking for updates…"),
      .done("Atlas is up to date."),
      .available("Atlas 0.2.0 is available."),
      .installing("Installing Atlas 0.2.0…"),
      .problem("Atlas couldn't check for updates."),
      .misplaced("Move Atlas to the Applications folder, then try again."),
      .note("Atlas has no public release yet. Check [atlas.insanearts.io](https://atlas.insanearts.io) for news."),
    ]
    let heights = try statuses.map { status in
      try Self.size(
        AboutPane(state: SettingsAboutState(version: "Version 0.1.0", update: .check, status: status)) { _ in }
          .frame(width: SettingsLayout.formWidth)
      )
      .height
    }
    #expect(Set(heights).count == 1, "\(heights)")
  }

  @Test("About's update button keeps one width as it checks, finds a newer version and installs")
  @MainActor
  func theUpdateButtonKeepsItsWidth() throws {
    let widths = try [SettingsUpdateButton.check, .checking, .install, .installing].map { update in
      try Self.size(AboutUpdateButton(update: update) { _ in }.fixedSize()).width
    }
    #expect(Set(widths).count == 1, "\(widths)")
  }

  @Test("About's note makes its web address a live link")
  func theNoteLinksItsAddress() {
    let note = AboutUpdateStatus.linked("Check [atlas.insanearts.io](https://atlas.insanearts.io) for news.")
    #expect(String(note.characters) == "Check atlas.insanearts.io for news.")
    #expect(note.runs.contains { $0.link == URL(string: "https://atlas.insanearts.io") })
  }
}
