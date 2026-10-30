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
      .background(SettingsSketchLayout.windowFill)
      .clipShape(RoundedRectangle(cornerRadius: SettingsSketchLayout.windowRadius, style: .continuous))
      .overlay(
        RoundedRectangle(cornerRadius: SettingsSketchLayout.windowRadius, style: .continuous)
          .strokeBorder(SettingsSketchLayout.edge, lineWidth: SettingsSketchLayout.edgeWidth)
      )
    }

    /// The sidebar: a column of bars, one picked.
    private var sidebar: some View {
      VStack(alignment: .leading, spacing: SettingsSketchLayout.sidebarGap) {
        ForEach(0..<SettingsSketchLayout.sidebarRows, id: \.self) { index in
          RoundedRectangle(cornerRadius: SettingsSketchLayout.barRadius, style: .continuous)
            .fill(index == SettingsSketchLayout.pickedRow ? SettingsSketchLayout.pickedBar : SettingsSketchLayout.bar)
            .frame(
              width: index.isMultiple(of: 3) ? SettingsSketchLayout.shortBar : SettingsSketchLayout.longBar,
              height: SettingsSketchLayout.barHeight
            )
        }
        Spacer()
      }
      .padding(.top, SettingsSketchLayout.sidebarTop)
      .padding(.horizontal, SettingsSketchLayout.sidebarInset)
      .frame(width: SettingsSketchLayout.sidebarWidth)
      .frame(maxHeight: .infinity)
      .background(SettingsSketchLayout.sidebarFill)
    }

    /// The page: its name over a group of apps, each with its switch.
    private var content: some View {
      VStack(alignment: .leading, spacing: SettingsSketchLayout.pageGap) {
        Text(page)
          .font(.system(size: SettingsSketchLayout.titleSize, weight: .bold))
          .foregroundStyle(SettingsSketchLayout.title)
        VStack(spacing: .zero) {
          ForEach(0..<SettingsSketchLayout.appRows, id: \.self) { _ in
            app
          }
        }
        .background(
          RoundedRectangle(cornerRadius: SettingsSketchLayout.groupRadius, style: .continuous)
            .fill(SettingsSketchLayout.groupFill)
        )
        Spacer()
      }
      .padding(.top, SettingsSketchLayout.pageTop)
      .padding(.horizontal, SettingsSketchLayout.pageInset)
    }

    /// One app in the list: its icon, its name and its switch, as plain shapes.
    private var app: some View {
      HStack(spacing: SettingsSketchLayout.appGap) {
        RoundedRectangle(cornerRadius: SettingsSketchLayout.iconRadius, style: .continuous)
          .fill(SettingsSketchLayout.icon)
          .frame(width: SettingsSketchLayout.iconSide, height: SettingsSketchLayout.iconSide)
        RoundedRectangle(cornerRadius: SettingsSketchLayout.nameRadius, style: .continuous)
          .fill(SettingsSketchLayout.name)
          .frame(width: SettingsSketchLayout.nameWidth, height: SettingsSketchLayout.nameHeight)
        Spacer()
        Capsule()
          .fill(SettingsSketchLayout.name)
          .frame(width: SettingsSketchLayout.switchWidth, height: SettingsSketchLayout.switchHeight)
      }
      .padding(.horizontal, SettingsSketchLayout.appInset)
      .frame(height: SettingsSketchLayout.appHeight)
    }
  }

#endif
