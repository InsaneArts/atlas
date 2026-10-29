#if DEBUG
  import AtlasUI
  import SwiftUI

  /// A sample System Settings window for the docked renders: a sidebar of bars, the page's name, and a list of apps
  /// with their switches, drawn plainly so it reads as the place the panel docks beside, never as the real window. It
  /// draws in the render's appearance, as the real window would.
  struct SettingsSketch: View {
    /// The page's name, as macOS names it.
    let page: String

    /// The window: its sidebar, then its page.
    var body: some View {
      HStack(spacing: .zero) {
        sidebar
        content
      }
      .background(Layout.windowFill)
      .clipShape(RoundedRectangle(cornerRadius: Layout.windowRadius))
      .overlay(
        RoundedRectangle(cornerRadius: Layout.windowRadius)
          .strokeBorder(Layout.edge, lineWidth: Layout.edgeWidth)
      )
    }

    /// The sidebar: a column of bars, one picked.
    private var sidebar: some View {
      VStack(alignment: .leading, spacing: Layout.sidebarGap) {
        ForEach(0..<Layout.sidebarRows, id: \.self) { index in
          RoundedRectangle(cornerRadius: Layout.barRadius)
            .fill(index == Layout.pickedRow ? Layout.pickedBar : Layout.bar)
            .frame(width: index.isMultiple(of: 3) ? Layout.shortBar : Layout.longBar, height: Layout.barHeight)
        }
        Spacer()
      }
      .padding(.top, Layout.sidebarTop)
      .padding(.horizontal, Layout.sidebarInset)
      .frame(width: Layout.sidebarWidth)
      .frame(maxHeight: .infinity)
      .background(Layout.sidebarFill)
    }

    /// The page: its name over a group of apps, each with its switch.
    private var content: some View {
      VStack(alignment: .leading, spacing: Layout.pageGap) {
        Text(page)
          .font(.system(size: Layout.titleSize, weight: .bold))
          .foregroundStyle(Layout.title)
        VStack(spacing: .zero) {
          ForEach(0..<Layout.appRows, id: \.self) { _ in
            app
          }
        }
        .background(
          RoundedRectangle(cornerRadius: Layout.groupRadius)
            .fill(Layout.groupFill)
        )
        Spacer()
      }
      .padding(.top, Layout.pageTop)
      .padding(.horizontal, Layout.pageInset)
    }

    /// One app in the list: its icon, its name and its switch, as plain shapes.
    private var app: some View {
      HStack(spacing: Layout.appGap) {
        RoundedRectangle(cornerRadius: Layout.iconRadius)
          .fill(Layout.icon)
          .frame(width: Layout.iconSide, height: Layout.iconSide)
        RoundedRectangle(cornerRadius: Layout.nameRadius)
          .fill(Layout.name)
          .frame(width: Layout.nameWidth, height: Layout.nameHeight)
        Spacer()
        Capsule()
          .fill(Layout.name)
          .frame(width: Layout.switchWidth, height: Layout.switchHeight)
      }
      .padding(.horizontal, Layout.appInset)
      .frame(height: Layout.appHeight)
    }
  }

  /// The sketch's own geometry and paint: a rough likeness of System Settings' window in dark mode, measured by eye,
  /// which no Atlas surface shares, so none of it is a design token.
  private enum Layout {
    /// 12: the window's corners.
    static let windowRadius: CGFloat = 12
    /// 0.5: the window's edge.
    static let edgeWidth: CGFloat = 0.5
    /// 10: the sidebar's bars.
    static let sidebarRows = 10
    /// The seventh bar, picked as Privacy & Security is.
    static let pickedRow = 6
    /// 10: between the sidebar's bars.
    static let sidebarGap: CGFloat = 10
    /// 4: a bar's corners.
    static let barRadius: CGFloat = 4
    /// 120: every third bar, shorter, as a short name is.
    static let shortBar: CGFloat = 120
    /// 150: the other bars.
    static let longBar: CGFloat = 150
    /// 14: a bar's height, a line of the sidebar's text.
    static let barHeight: CGFloat = 14
    /// 52: the sidebar's first bar under the window's top, below its buttons and search field.
    static let sidebarTop: CGFloat = 52
    /// 16: the sidebar's sides.
    static let sidebarInset: CGFloat = 16
    /// 200: the sidebar's width.
    static let sidebarWidth: CGFloat = 200
    /// 16: between the page's name and its group.
    static let pageGap: CGFloat = 16
    /// 15: the page's name, as System Settings sets it.
    static let titleSize: CGFloat = 15
    /// 3: the apps in the group.
    static let appRows = 3
    /// 10: the group's corners.
    static let groupRadius: CGFloat = 10
    /// 48: the page's name under the window's top.
    static let pageTop: CGFloat = 48
    /// 22: the page's sides.
    static let pageInset: CGFloat = 22
    /// 10: between an app's icon, name and switch.
    static let appGap: CGFloat = 10
    /// 5: an app icon's corners.
    static let iconRadius: CGFloat = 5
    /// 22: an app icon's side.
    static let iconSide: CGFloat = 22
    /// 3: an app name's corners.
    static let nameRadius: CGFloat = 3
    /// 110: an app name's width.
    static let nameWidth: CGFloat = 110
    /// 10: an app name's height.
    static let nameHeight: CGFloat = 10
    /// 32: a switch's width.
    static let switchWidth: CGFloat = 32
    /// 18: a switch's height.
    static let switchHeight: CGFloat = 18
    /// 12: an app row's sides.
    static let appInset: CGFloat = 12
    /// 40: an app row's height.
    static let appHeight: CGFloat = 40
    /// The window's ground: the system's window background, light or dark as the render is.
    static let windowFill = Color(nsColor: .windowBackgroundColor)
    /// The window's edge.
    static let edge = Color.primary
      .opacity(0.12)
    /// The sidebar's ground, a shade off the window's.
    static let sidebarFill = Color.primary
      .opacity(0.04)
    /// A sidebar bar.
    static let bar = Color.primary
      .opacity(0.08)
    /// The picked sidebar bar.
    static let pickedBar = Color.primary
      .opacity(0.22)
    /// The page's name.
    static let title = Color.primary
      .opacity(0.85)
    /// The group's ground.
    static let groupFill = Color.primary
      .opacity(0.05)
    /// An app's icon.
    static let icon = Color.primary
      .opacity(0.14)
    /// An app's name and its switch.
    static let name = Color.primary
      .opacity(0.12)
  }
#endif
